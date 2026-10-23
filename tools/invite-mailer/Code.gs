// Code.gs -- the Apps Script web app that sends CaveCAD invite emails from cavecad.app@gmail.com.
// Script properties: KEY (shared with the CaveCAD server's MAILER_KEY), SITE (https://cave.schonegg.stream).

function doPost(e) {
  var props = PropertiesService.getScriptProperties();
  var p = {};
  try { p = JSON.parse(e.postData.contents); } catch (err) { return answer({ ok: false, error: "json" }); }
  var cache = CacheService.getScriptCache();
  var hk = InviteLogic.hourKey(new Date());
  var count = Number(cache.get(hk) || 0);
  var why = InviteLogic.check({ key: p.key, expected: props.getProperty("KEY"), to: p.to, link: p.link, site: props.getProperty("SITE"), countThisHour: count });
  if (why) return answer({ ok: false, error: why });
  var m = InviteLogic.message(p);
  MailApp.sendEmail({ to: p.to, subject: m.subject, body: m.body, name: "CaveCAD" });
  cache.put(hk, String(count + 1), 3600);
  return answer({ ok: true });
}

function answer(o) {
  return ContentService.createTextOutput(JSON.stringify(o)).setMimeType(ContentService.MimeType.JSON);
}
