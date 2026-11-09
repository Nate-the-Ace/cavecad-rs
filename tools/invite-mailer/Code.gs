// Code.gs -- the Apps Script web app that sends CaveCAD invite emails from cavecad.app@gmail.com.
// Script properties: KEY (shared with the CaveCAD server's MAILER_KEY), SITE (https://cavecad.com).

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
    var why = InviteLogic.check({ key: p.key, expected: props.getProperty("KEY"), to: p.to, link: p.link, kind: p.kind, code: p.code, site: props.getProperty("SITE"), countThisHour: count });
    if (why) return answer({ ok: false, error: why });
    var m = InviteLogic.message(p);
    GmailApp.sendEmail(p.to, m.subject, m.body, { from: "support@cavecad.com", name: "CaveCAD" });
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
  props.setProperty("SITE", "https://cavecad.com");
  GmailApp.getAliases(); // asks for the Gmail permission now, not on the first email
  Logger.log("Set up. Copy KEY from Project Settings > Script properties into the server.");
}

function answer(o) {
  return ContentService.createTextOutput(JSON.stringify(o)).setMimeType(ContentService.MimeType.JSON);
}
