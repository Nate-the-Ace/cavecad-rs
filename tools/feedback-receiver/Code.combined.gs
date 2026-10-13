// Logic.gs -- the receiver's decisions, free of Google services so node
// can test them (tools/feedback-receiver/test). Apps Script loads every .gs
// file into one global scope; node loads this one through module.exports.

var FeedbackLogic = {
  MAX_BODY: 40 * 1024 * 1024,
  MAX_PER_HOUR: 30,
  CLOSED_GRACE_DAYS: 30,
  MAX_AGE_DAYS: 365,
  MAX_MAIL_BYTES: 20 * 1024 * 1024,
  CLOSED_STATES: ["Fixed", "Won't fix"],
  COLUMNS: ["Received", "ID", "Type", "Summary", "Email", "CaveCAD version", "OS",
            "Attachments", "Size", "Folder link", "Folder ID", "Status", "Closed", "Notes"],

  /** null when accepted, else "key" | "too-big" | "busy". */
  check: function (p) {
    if (!p.expected || p.key !== p.expected) return "key";
    if (p.bodyLength > FeedbackLogic.MAX_BODY) return "too-big";
    if (p.countThisHour >= FeedbackLogic.MAX_PER_HOUR) return "busy";
    return null;
  },

  hourKey: function (date) { return "n-" + date.toISOString().slice(0, 13); },

  TYPES: ["bug", "idea", "question"],

  typeLabel: function (t) {
    t = String(t || "bug").toLowerCase();
    if (FeedbackLogic.TYPES.indexOf(t) === -1) t = "bug";
    return t.charAt(0).toUpperCase() + t.slice(1);
  },

  /** Prefix values Sheets would treat as a formula (=+-@ and tab/CR) with a quote, so a
   * reporter-controlled string (summary, email, ...) can never inject a formula. */
  escapeFormula: function (s) {
    if (typeof s !== "string") return s;
    return /^[=+\-@\t\r]/.test(s) ? "'" + s : s;
  },

  folderName: function (report, date) {
    var s = String(report.summary || "").replace(/[\/\\]/g, "-").replace(/\s+/g, " ").trim().slice(0, 60);
    return date.toISOString().slice(0, 10) + " " + FeedbackLogic.typeLabel(report.type) + " — " + s + " (" + report.id + ")";
  },

  row: function (report, received, folderUrl, folderId, bytes) {
    // Reporter-controlled fields must be coerced to plain strings before escaping --
    // e.g. an array like ["=HYPERLINK(...)"] stringifies harmlessly via String(),
    // but would otherwise slip past a typeof-string check untouched.
    var v = {
      "Received": received, "ID": String(report.id || ""), "Type": FeedbackLogic.typeLabel(report.type),
      "Summary": String(report.summary || ""), "Email": String(report.email || ""),
      "CaveCAD version": String((report.cavecad && report.cavecad.version) || ""),
      "OS": String(report.os || ""),
      "Attachments": (report.attachments || []).length, "Size": bytes, "Folder link": folderUrl,
      "Folder ID": folderId, "Status": "New", "Closed": "", "Notes": ""
    };
    return FeedbackLogic.COLUMNS.map(function (c) { return FeedbackLogic.escapeFormula(v[c]); });
  },

  /** rows: [{received, status, closed}] -> indexes due for purging. */
  purgeDue: function (rows, now) {
    var day = 86400000, out = [];
    rows.forEach(function (r, i) {
      if (r.status === "Purged") return;
      var old = r.received instanceof Date && now - r.received >= FeedbackLogic.MAX_AGE_DAYS * day;
      var closed = FeedbackLogic.CLOSED_STATES.indexOf(r.status) >= 0 && r.closed instanceof Date
        && now - r.closed >= FeedbackLogic.CLOSED_GRACE_DAYS * day;
      if (old || closed) out.push(i);
    });
    return out;
  },

  stripPath: function (name) { return String(name).replace(/^(\.\/)+/, ""); },

  /** false for a zip directory entry (tar/ditto write "logs/", "./logs/", "./") or an
   * empty name; true for an actual file entry. */
  isFileEntry: function (name) {
    var s = FeedbackLogic.stripPath(name);
    return s.length > 0 && s.charAt(s.length - 1) !== "/";
  },

  /** True for a report file worth attaching to the email: the log, a screenshot, the written report. */
  isMailable: function (name) {
    var n = FeedbackLogic.stripPath(name).toLowerCase();
    if (n === "report.json") return false;
    return /(^|\/)(log\.txt|feedback\.txt)$/.test(n) || /^logs\//.test(n) || /^[^\/]+\.(png|jpe?g|webp)$/.test(n);
  },

  subject: function (report) {
    return "[CaveCAD Feedback] " + FeedbackLogic.typeLabel(report.type) + ": " + report.summary;
  }
};

// Code.gs -- CaveCAD's feedback receiver. Deployed as a web app under
// cavecad.app@gmail.com ("Execute as: Me", "Who has access: Anyone").
// Script properties: KEY, FOLDER_ID, SHEET_ID. See README.md.

function doPost(e) {
  var props = PropertiesService.getScriptProperties();
  var cache = CacheService.getScriptCache();
  var hk = FeedbackLogic.hourKey(new Date());
  var n = Number(cache.get(hk) || 0);
  var err = FeedbackLogic.check({
    key: e.parameter.k, expected: props.getProperty("KEY"),
    bodyLength: e.postData ? e.postData.length : 0, countThisHour: n
  });
  if (err) return reply({ ok: false, error: err });
  if (!e.postData || !e.postData.contents) return reply({ ok: false, error: "no-report" });
  cache.put(hk, String(n + 1), 3600);

  var bytes, zip, files, report = null;
  try {
    // The macOS client's base64 file ends with a trailing newline; some transports
    // may also wrap/pad with other whitespace, so strip it all before decoding.
    bytes = Utilities.base64Decode(e.postData.contents.replace(/\s+/g, ""));
    zip = Utilities.newBlob(bytes, "application/zip", "report.zip");
    files = Utilities.unzip(zip);
    files.forEach(function (f) {
      if (FeedbackLogic.stripPath(f.getName()) === "report.json") report = JSON.parse(f.getDataAsString());
    });
  } catch (parseErr) {
    return reply({ ok: false, error: "bad-zip" });
  }
  if (!report || !/^[0-9a-f]{6}$/.test(String(report.id))) return reply({ ok: false, error: "no-report" });

  // From here a Drive folder may already exist for this report: any later failure must
  // still reply ok so the client does not resend and create duplicate folders/rows/mail.
  var now = new Date();
  var root = DriveApp.getFolderById(props.getProperty("FOLDER_ID"));
  var folder = root.createFolder(FeedbackLogic.folderName(report, now));
  folder.createFile(zip.setName("feedback-" + report.id + ".zip"));
  files.forEach(function (f) {
    // Each entry is independent: one bad name/permission must not throw and abort the
    // loop, which would leave the client resending and duplicating the whole folder.
    try {
      var name = FeedbackLogic.stripPath(f.getName());
      if (!FeedbackLogic.isFileEntry(name)) return; // tar/ditto directory entry ("logs/", "./logs/", "./")
      var parts = name.split("/").filter(function (p) { return p && p !== "." && p !== ".."; });
      var leaf = parts.pop();
      if (!leaf) return;
      var dir = folder;
      parts.forEach(function (p) {
        var it = dir.getFoldersByName(p);
        dir = it.hasNext() ? it.next() : dir.createFolder(p);
      });
      dir.createFile(f.setName(leaf));
    } catch (fileErr) {
      console.error("feedback " + report.id + ": attachment " + f.getName() + " failed: " + fileErr);
    }
  });

  try {
    var sheet = SpreadsheetApp.openById(props.getProperty("SHEET_ID")).getSheets()[0];
    if (sheet.getLastRow() === 0) sheet.appendRow(FeedbackLogic.COLUMNS);
    sheet.appendRow(FeedbackLogic.row(report, now, folder.getUrl(), folder.getId(), bytes.length));
  } catch (sheetErr) {
    console.error("feedback " + report.id + ": sheet append failed: " + sheetErr);
  }

  try {
    // The email carries the report's own files (the log, the screenshot, the written report) so it can be
    // read without opening Drive. report.json and anything over the mail limit stay in the Drive folder only.
    var attach = [], total = 0;
    files.forEach(function (f) {
      var name = FeedbackLogic.stripPath(f.getName());
      if (!FeedbackLogic.isFileEntry(name) || !FeedbackLogic.isMailable(name)) return;
      var size = f.getBytes().length;
      if (total + size > FeedbackLogic.MAX_MAIL_BYTES) return;
      total += size;
      attach.push(f.copyBlob().setName(name.split("/").pop()));
    });
    MailApp.sendEmail({
      to: Session.getEffectiveUser().getEmail(),
      subject: FeedbackLogic.subject(report),
      body: report.summary + "\n\n" + (report.cavecad ? report.cavecad.version : "") + " | " + (report.os || "")
        + "\n\nAttached: " + (attach.map(function (a) { return a.getName(); }).join(", ") || "(nothing)")
        + "\n\nEverything is also in Drive: " + folder.getUrl(),
      attachments: attach
    });
  } catch (mailErr) {
    console.error("feedback " + report.id + ": mail send failed: " + mailErr);
  }

  return reply({ ok: true, id: report.id });
}

function reply(o) {
  return ContentService.createTextOutput(JSON.stringify(o)).setMimeType(ContentService.MimeType.JSON);
}

/** Installable on-edit trigger: Status -> Fixed/Won't fix stamps Closed; anything else clears it.
 * Handles a paste/fill spanning multiple rows and/or columns, not just a single-cell edit.
 * Always reads Status back from the sheet -- e.value is undefined for a paste or a
 * Delete-key clear, so relying on it silently skips exactly those two common edits. */
function onStatusEdit(e) {
  var sheet = e.range.getSheet();
  if (sheet.getIndex() !== 1) return; // triage sheet only
  var statusCol = FeedbackLogic.COLUMNS.indexOf("Status") + 1;
  var closedCol = FeedbackLogic.COLUMNS.indexOf("Closed") + 1;
  var firstCol = e.range.getColumn(), lastCol = firstCol + e.range.getNumColumns() - 1;
  if (statusCol < firstCol || statusCol > lastCol) return;

  var firstRow = e.range.getRow(), numRows = e.range.getNumRows();
  for (var i = 0; i < numRows; i++) {
    var row = firstRow + i;
    if (row === 1) continue; // header
    var status = String(sheet.getRange(row, statusCol).getValue());
    sheet.getRange(row, closedCol).setValue(FeedbackLogic.CLOSED_STATES.indexOf(status) >= 0 ? new Date() : "");
  }
}

/** Daily time-driven trigger: trash due report folders, mark rows Purged. */
function purgeOld() {
  var props = PropertiesService.getScriptProperties();
  var sheet = SpreadsheetApp.openById(props.getProperty("SHEET_ID")).getSheets()[0];
  if (sheet.getLastRow() < 2) return;
  var C = FeedbackLogic.COLUMNS;
  var values = sheet.getRange(2, 1, sheet.getLastRow() - 1, C.length).getValues();
  var rows = values.map(function (v) {
    return { received: v[C.indexOf("Received")], status: String(v[C.indexOf("Status")]), closed: v[C.indexOf("Closed")] };
  });
  FeedbackLogic.purgeDue(rows, new Date()).forEach(function (i) {
    try { DriveApp.getFolderById(values[i][C.indexOf("Folder ID")]).setTrashed(true); } catch (err) { /* already gone */ }
    sheet.getRange(i + 2, C.indexOf("Status") + 1).setValue("Purged");
  });
}

/**
 * Run once from the editor (after FOLDER_ID and SHEET_ID are set): creates
 * the on-edit trigger for onStatusEdit and the daily trigger for purgeOld,
 * replacing any earlier copies so running it twice is harmless.
 */
function installTriggers() {
  var sheetId = PropertiesService.getScriptProperties().getProperty("SHEET_ID");
  ScriptApp.getProjectTriggers().forEach(function (t) {
    var f = t.getHandlerFunction();
    if (f === "onStatusEdit" || f === "purgeOld") ScriptApp.deleteTrigger(t);
  });
  ScriptApp.newTrigger("onStatusEdit").forSpreadsheet(sheetId).onEdit().create();
  ScriptApp.newTrigger("purgeOld").timeBased().everyDays(1).atHour(3).create();
}
