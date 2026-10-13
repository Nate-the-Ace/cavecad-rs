// Code.gs -- CaveCAD's signaling mailbox. Deployed as a web app under cavecad.app@gmail.com
// ("Execute as: Me", "Who has access: Anyone"), as its own Apps Script project (not inside the feedback
// receiver, which has its own doPost). Script property: KEY. See README.md. All decisions are in Logic.gs.

function reply(obj) {
  return ContentService.createTextOutput(JSON.stringify(obj)).setMimeType(ContentService.MimeType.JSON);
}

function handle(e, body) {
  var p = e.parameter || {};
  var lock = LockService.getScriptLock();
  var out = SignalLogic.run({
    cache: CacheService.getScriptCache(),
    expected: PropertiesService.getScriptProperties().getProperty("KEY"),
    now: new Date(),
    locked: function (fn) {
      lock.waitLock(5000);
      try { fn(); } finally { lock.releaseLock(); }
    }
  }, { op: p.op, key: p.k, session: p.s, name: p.n, prefix: p.p, body: body });
  return reply(out);
}

/** Reads: op=get (one note) and op=list (the notes with a name prefix). */
function doGet(e) {
  return handle(e, "");
}

/** Writes: op=put (the note is the request body, plain text) and op=del. */
function doPost(e) {
  return handle(e, e.postData && e.postData.contents ? e.postData.contents : "");
}
