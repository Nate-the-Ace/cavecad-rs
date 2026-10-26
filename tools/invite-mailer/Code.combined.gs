// Code.combined.gs -- Logic.gs and Code.gs in one file, for pasting into a single Apps Script file.
// Made from those two; edit them, not this.

// Logic.gs -- the invite mailer's decisions, free of Google services so node can test them
// (tools/invite-mailer/test.mjs). Apps Script loads every .gs file into one global scope; node loads
// this one through module.exports.

var InviteLogic = {
  MAX_PER_HOUR: 40,

  /** null when the request may be sent, else "key" | "address" | "link" | "busy". */
  check: function (p) {
    if (!p.expected || p.key !== p.expected) return "key";
    if (!/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(String(p.to || ""))) return "address";
    if (String(p.link || "").indexOf(p.site + "/?signup=") !== 0) return "link";
    if (p.countThisHour >= InviteLogic.MAX_PER_HOUR) return "busy";
    return null;
  },

  hourKey: function (date) { return "n-" + date.toISOString().slice(0, 13); },

  /** The email itself: plain text, no tracking. */
  message: function (p) {
    var inviter = String(p.inviter || "Someone").slice(0, 60);
    return {
      subject: inviter + " invited you to CaveCAD",
      body: inviter + " invited you to CaveCAD, the cave survey and mapping app.\n\n" +
        "Sign in with " + p.to + " (Google). The link works once, for that address, for 7 days.\n\n" +
        p.link + "\n\n" +
        "CaveCAD stores caves on its server and syncs them to your devices.\n"
    };
  }
};


// Code.gs -- the Apps Script web app that sends CaveCAD invite emails from cavecad.app@gmail.com.
// Script properties: KEY (shared with the CaveCAD server's MAILER_KEY), SITE (https://cave.schonegg.stream).

function doPost(e) {
  var props = PropertiesService.getScriptProperties();
  var p = {};
  try { p = JSON.parse(e.postData.contents); } catch (err) { return answer({ ok: false, error: "json" }); }
  var cache = CacheService.getScriptCache();
  var hk = InviteLogic.hourKey(new Date());
  var lock = LockService.getScriptLock();
  lock.waitLock(10000);
  try {
    var count = Number(cache.get(hk) || 0);
    var why = InviteLogic.check({ key: p.key, expected: props.getProperty("KEY"), to: p.to, link: p.link, site: props.getProperty("SITE"), countThisHour: count });
    if (why) return answer({ ok: false, error: why });
    var m = InviteLogic.message(p);
    MailApp.sendEmail({ to: p.to, subject: m.subject, body: m.body, name: "CaveCAD" });
    cache.put(hk, String(count + 1), 3600);
    return answer({ ok: true });
  } finally {
    lock.releaseLock();
  }
}

/** Run once from the editor: makes the shared KEY (kept here and in the server's MAILER_KEY) and sets SITE. */
function setUp() {
  var props = PropertiesService.getScriptProperties();
  if (!props.getProperty("KEY")) props.setProperty("KEY", Utilities.getUuid().replace(/-/g, "") + Utilities.getUuid().replace(/-/g, ""));
  props.setProperty("SITE", "https://cave.schonegg.stream");
  MailApp.getRemainingDailyQuota(); // asks for the send-email permission now, not on the first invite
  Logger.log("Set up. Copy KEY from Project Settings > Script properties into the server.");
}

function answer(o) {
  return ContentService.createTextOutput(JSON.stringify(o)).setMimeType(ContentService.MimeType.JSON);
}
