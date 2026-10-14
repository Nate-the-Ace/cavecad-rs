// Logic.gs -- the signaling mailbox's decisions and storage rules, free of Google services so node can test
// them (tools/signal-mailbox/test). Apps Script loads every .gs file into one global scope; node loads this
// one through module.exports. `run` takes the cache it should use, so the tests give it a fake one.
//
// The mailbox lets CaveCAD pages that have never met pass each other the short notes a direct (WebRTC)
// connection needs to start: "I am here", "here is an offer for you", "here is my answer". Only those three
// kinds of note can be stored, they expire on their own, and the mailbox never sees a cave: after the
// handshake the pages talk to each other directly.

var SignalLogic = {
  /** Biggest note, in characters (an offer or answer code is a few thousand). */
  MAX_VALUE: 12000,
  /** Most notes one session can hold. */
  MAX_NAMES: 400,
  /** Most calls to the mailbox per minute, all sessions together. */
  MAX_PER_MINUTE: 900,
  /** How long a note lives, in seconds. */
  TTL: 1800,

  /** A session id: the part of a class link that names the class, long enough that nobody can guess it. */
  validSession: function (s) { return typeof s === "string" && /^[A-Za-z0-9_-]{8,40}$/.test(s); },

  /** A note's name: `hello:`, `offer:` or `answer:` and the id of one student's attempt. Nothing else is stored. */
  validName: function (n) { return typeof n === "string" && /^(hello|offer|answer):[A-Za-z0-9_-]{4,40}$/.test(n); },

  minuteKey: function (date) { return "m-" + date.toISOString().slice(0, 16); },

  cacheKey: function (session, name) { return "sig:" + session + ":" + name; },
  indexKey: function (session) { return "idx:" + session; },

  /** null when accepted, else "key" | "session" | "name" | "too-big" | "busy". */
  check: function (p) {
    if (!p.expected || p.key !== p.expected) return "key";
    if (!SignalLogic.validSession(p.session)) return "session";
    if (p.needsName && !SignalLogic.validName(p.name)) return "name";
    if (p.bodyLength > SignalLogic.MAX_VALUE) return "too-big";
    if (p.countThisMinute >= SignalLogic.MAX_PER_MINUTE) return "busy";
    return null;
  },

  parseIndex: function (text) {
    try {
      var a = JSON.parse(text || "[]");
      return Array.isArray(a) ? a.filter(SignalLogic.validName) : [];
    } catch (e) { return []; }
  },

  withName: function (index, name) {
    if (index.indexOf(name) >= 0) return index;
    var next = index.concat([name]);
    return next.length > SignalLogic.MAX_NAMES ? next.slice(next.length - SignalLogic.MAX_NAMES) : next;
  },

  withoutName: function (index, name) { return index.filter(function (n) { return n !== name; }); },

  matching: function (index, prefix) {
    prefix = String(prefix || "");
    return index.filter(function (n) { return n.indexOf(prefix) === 0; });
  },

  /**
   * Does one call. `env`: { cache: { get, put(key, value, seconds), getAll(keys), remove }, expected: the key,
   * now: Date, locked(fn): runs fn with writers kept apart }. `p`: { op, key, session, name, prefix, body }.
   * Returns the reply as plain data.
   */
  run: function (env, p) {
    var op = String(p.op || "");
    var needsName = op === "put" || op === "get" || op === "del";
    var mk = SignalLogic.minuteKey(env.now);
    var n = Number(env.cache.get(mk) || 0);
    var err = SignalLogic.check({
      key: p.key, expected: env.expected, session: p.session, name: p.name, needsName: needsName,
      bodyLength: p.body ? p.body.length : 0, countThisMinute: n
    });
    if (err) return { ok: false, error: err };
    if (["put", "get", "list", "del"].indexOf(op) < 0) return { ok: false, error: "op" };
    env.cache.put(mk, String(n + 1), 120);

    var ik = SignalLogic.indexKey(p.session);
    if (op === "put") {
      if (!p.body) return { ok: false, error: "empty" };
      env.locked(function () {
        env.cache.put(SignalLogic.cacheKey(p.session, p.name), p.body, SignalLogic.TTL);
        var index = SignalLogic.withName(SignalLogic.parseIndex(env.cache.get(ik)), p.name);
        env.cache.put(ik, JSON.stringify(index), SignalLogic.TTL);
      });
      return { ok: true };
    }
    if (op === "get") {
      var v = env.cache.get(SignalLogic.cacheKey(p.session, p.name));
      return { ok: true, value: v === null || v === undefined ? null : v };
    }
    if (op === "del") {
      env.locked(function () {
        env.cache.remove(SignalLogic.cacheKey(p.session, p.name));
        env.cache.put(ik, JSON.stringify(SignalLogic.withoutName(SignalLogic.parseIndex(env.cache.get(ik)), p.name)), SignalLogic.TTL);
      });
      return { ok: true };
    }
    // list: the notes whose name starts with the prefix, with their values (they are small)
    var names = SignalLogic.matching(SignalLogic.parseIndex(env.cache.get(ik)), p.prefix);
    var keys = names.map(function (m) { return SignalLogic.cacheKey(p.session, m); });
    var found = keys.length ? env.cache.getAll(keys) : {};
    var items = [];
    names.forEach(function (m) {
      var val = found[SignalLogic.cacheKey(p.session, m)];
      if (val !== undefined && val !== null) items.push({ n: m, v: val }); // an expired note drops out
    });
    return { ok: true, items: items };
  }
};

if (typeof module !== "undefined") { module.exports = SignalLogic; }

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
