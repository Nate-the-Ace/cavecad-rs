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
        "CaveCAD never saves your cave data on its server in a form it can read: your caves stay on your " +
        "devices and in the drives you connect.\n"
    };
  }
};

if (typeof module !== "undefined") module.exports = InviteLogic;
