// A classroom: one teacher shares a cave, and any number of students watch it.
//
//   TEACHER  starts the class (a room on the relay, made as a class), sends students a link, and works as
//            usual. Everything the teacher looks at (the panels and where they are, the ribbon's tab, each
//            view's camera, the trip, run and station on show, the tool picked) is mirrored to every student
//            as it changes. The teacher sees each student's pointer, who has a hand up, and the chat.
//   STUDENT  opens the link, gives a name, and is put on the teacher's view. A student cannot change the cave
//            (the engine refuses writes, the relay refuses ops, and the page ignores clicks and keys), cannot
//            steer the view, but is seen: their pointer shows to the teacher. A raised hand and a line of
//            chat are the way to ask a question.
//
// Every hand and every line said is logged by the relay (in memory, and to a file on the server if it keeps
// one) and the teacher can download the whole transcript. A class lives on the room relay (the dev or Docker
// server's /sync), so it needs that server; the serverless peer sessions are for working together.
import { engine } from './engine';
import { cave, changed, filter } from './cave.svelte';
import { applyMirror, captureMirror, forgetMirror, registerMirror } from './classroom-mirror';
import { clearScans } from './scans.svelte';
import { classLink, desktopLink, liveLink, mergeEntries, readClassLink, readJoinLink, readLiveLink, transcript, transcriptName, type ChatEntry, type Hand } from './classroom-log';
import { readOnly } from './readonly';
import { shelf } from './shelf.svelte';
import { joinViaLink, openClassLink, p2p, sendClass, startClassHosting } from './p2p.svelte';
import { shareBase } from './share-base';
import { loadEngine } from './engine';
import { configuredMailbox, httpMailbox, mailboxConfig, type Mailbox } from './signal';
import { createInvite, hasSavedName, join, leave, live, me, rename, sendRoom, setRoomHooks } from './sync.svelte';

export const classroom = $state({
  /** What this device is in a class: nothing, the teacher, or a student. */
  role: 'none' as 'none' | 'teacher' | 'student',
  /** The class's name (the room). */
  name: '',
  /** When the class started (ms, the relay's clock). */
  started: 0,
  /** The teacher ended it: nothing more can be said. */
  ended: false,
  /** Everything said and every hand, in order. */
  /** The same link as one that opens the desktop app. */
  appLink: '',
  /** Students may not keep the cave (set by the teacher when the class starts). */
  private: true,
  chat: [] as ChatEntry[],
  /** Things the teacher should notice now: a hand going up, a line with an emoji in it. They show as cards on the screen. */
  alerts: [] as { id: number; kind: 'hand' | 'emoji'; whoId: string; who: string; text: string }[],
  /** A soft chime when a hand goes up (the teacher can turn it off). */
  chime: (() => {
    try {
      return localStorage.getItem('cavecad.chime') !== 'off';
    } catch {
      return true;
    }
  })(),
  /** Hands up now, longest waiting first. */
  hands: [] as Hand[],
  /** Lines said while the drawer was shut (for the badge). */
  unread: 0,
  /** The side drawer (chat, hands, people) is open. */
  drawer: true,
  /** A short notice from the relay ("Slow down a little."), shown for a moment. */
  notice: '',
  /** A class link this page was opened with, waiting for the student to give a name and join. */
  pending: null as { room: string; invite: string; live?: { session: string; mailbox: { endpoint: string; key: string } | null } } | null,
  /** The Start a Classroom dialog is open. */
  dialog: false,
  /** Why starting or joining failed, in words. */
  error: '',
  /** Connecting now (the Join or Start button was pressed). */
  busy: false,
  /** How this class is carried: on the room relay (a server and a link) or straight between devices (a link through the
   *  mailbox, or codes). */
  transport: 'relay' as 'relay' | 'p2p',
  /** The link students open, for a direct class made with a link (a relay class gets its link from the relay's invite). */
  link: '',
});

/** Whether this device's hand is up. */
export const handIsUp = (): boolean => classroom.hands.some((h) => h.id === live.me);

/** The link students open, once the teacher has an invite. Empty until then. */
export function studentLink(): string {
  if (classroom.transport === 'p2p') return classroom.link;
  return live.made && classroom.name ? classLink(location.href, classroom.name, live.made) : '';
}

/** Reads a class link the page was opened with (and tidies the address bar). */
export function readClassLinkFromUrl() {
  const live = readLiveLink(location.search);
  const found = live ? { room: '', invite: '', live: { session: live.session, mailbox: live.mailbox } } : readClassLink(location.search);
  if (!found) return;
  history.replaceState(null, '', location.pathname + location.hash);
  acceptLink(found);
}

/** A join link has arrived (opened in the browser, pasted, or handed over by the desktop app): the page for it comes up.
 *  Someone who has used CaveCAD here before is joined at once, under the name they gave: opening the link is all it takes.
 *  A first-time visitor is asked for a name first. */
function acceptLink(found: NonNullable<typeof classroom.pending>) {
  classroom.pending = found;
  if ('live' in found && found.live && hasSavedName()) void loadEngine().then(() => classroom.pending && classroom.role === 'none' && joinClass(me.name));
}

/** A join link that opened the desktop app (`cavecad://join?...`). */
export function openJoinUrl(url: string) {
  const found = readJoinLink(url);
  if (found && classroom.role === 'none' && p2p.role === 'none') acceptLink({ room: '', invite: '', live: found });
}

/**
 * Someone pasted a join link (into the Cave Shelf, or any other copy of CaveCAD that cannot open an address): the
 * same page opens as when a link is opened in the browser. Gives back a sentence when it is not a join link.
 */
export function openLinkText(text: string): string | null {
  const found = readJoinLink(text);
  if (!found) return 'That is not a CaveCAD join link. Paste the whole link the host sent you.';
  classroom.error = '';
  acceptLink({ room: '', invite: '', live: found });
  return null;
}

// ---- getting the teacher's attention ---------------------------------------------------------------

let alertSeq = 0;
const EMOJI = /\p{Extended_Pictographic}/u;

/** A short soft two-note chime (no sound file; the browser makes it). Quietly does nothing where audio is not allowed. */
function chime() {
  if (!classroom.chime) return;
  try {
    const Ctx = window.AudioContext ?? (window as unknown as { webkitAudioContext?: typeof AudioContext }).webkitAudioContext;
    if (!Ctx) return;
    const ctx = new Ctx();
    const at = ctx.currentTime;
    [660, 880].forEach((freq, i) => {
      const o = ctx.createOscillator();
      const g = ctx.createGain();
      o.type = 'sine';
      o.frequency.value = freq;
      g.gain.setValueAtTime(0.0001, at + i * 0.14);
      g.gain.exponentialRampToValueAtTime(0.18, at + i * 0.14 + 0.02);
      g.gain.exponentialRampToValueAtTime(0.0001, at + i * 0.14 + 0.28);
      o.connect(g).connect(ctx.destination);
      o.start(at + i * 0.14);
      o.stop(at + i * 0.14 + 0.3);
    });
    setTimeout(() => void ctx.close(), 800);
  } catch {
    /* no sound is fine */
  }
}

export function setChime(on: boolean) {
  classroom.chime = on;
  try {
    localStorage.setItem('cavecad.chime', on ? 'on' : 'off');
  } catch {
    /* kept for this visit */
  }
}

/** Takes a card off the screen. */
export function dismissAlert(id: number) {
  classroom.alerts = classroom.alerts.filter((a) => a.id !== id);
}

let titleTimer: ReturnType<typeof setInterval> | undefined;
let plainTitle = '';
/** While someone's hand is up and this tab is out of sight, the tab's title flashes who it is. */
function flashTitle(who: string) {
  if (!document.hidden || titleTimer) return;
  plainTitle = document.title;
  let on = false;
  titleTimer = setInterval(() => {
    document.title = (on = !on) ? `✋ ${who} · ${plainTitle}` : plainTitle;
    if (!document.hidden) stopFlash();
  }, 900);
}
function stopFlash() {
  if (titleTimer) clearInterval(titleTimer);
  titleTimer = undefined;
  if (plainTitle) document.title = plainTitle;
}
if (typeof document !== 'undefined') document.addEventListener('visibilitychange', () => !document.hidden && stopFlash());

/** The teacher is told, loudly enough to notice, about a hand or an emoji from a student. */
function attend(e: ChatEntry) {
  if (classroom.role !== 'teacher' || e.id === live.me || !e.id) return;
  const who = e.name ?? 'A student';
  if (e.kind === 'hand' && e.text === 'raised a hand') {
    classroom.alerts = [...classroom.alerts.filter((a) => !(a.kind === 'hand' && a.whoId === e.id)), { id: ++alertSeq, kind: 'hand', whoId: e.id, who, text: '✋' }];
    chime();
    flashTitle(who);
  } else if (e.kind === 'say' && EMOJI.test(e.text)) {
    const id = ++alertSeq;
    classroom.alerts = [...classroom.alerts, { id, kind: 'emoji', whoId: e.id, who, text: e.text.slice(0, 60) }];
    setTimeout(() => dismissAlert(id), 7000);
  }
}

let intent: 'teacher' | 'student' | '' = '';

/** Sends a message to the class, whichever way it is carried. False when there is nobody to send it to. */
function transmit(msg: { t: string } & Record<string, unknown>): boolean {
  return classroom.transport === 'p2p' ? sendClass(msg) : sendRoom(msg);
}
let noticeTimer: ReturnType<typeof setTimeout> | undefined;
let beat: ReturnType<typeof setInterval> | undefined;
let lastSent = '';
let unblock: (() => void) | null = null;

// ---- starting and joining ------------------------------------------------------------------------

/** The teacher starts a class from the open cave. */
export function startClass(className: string, teacherName: string, via: 'relay' | 'link' = 'link', options: { private?: boolean } = {}) {
  const name = className.trim();
  if (!name) return void (classroom.error = 'Give the class a name.');
  if (!cave.hasCave) return void (classroom.error = 'Open or start a cave first: there is nothing to teach from yet.');
  classroom.error = '';
  if (teacherName.trim()) rename(teacherName.trim());
  if (via === 'link') {
    // Straight between devices: this page is the teacher and the hub. Students come in through a link (the mailbox passes
    // the first notes, then the devices connect directly) or with invite and reply codes.
    const box = configuredMailbox();
    if (!box) return void (classroom.error = 'This copy of CaveCAD has no class mailbox. Enter it below.');
    leave();
    classroom.transport = 'p2p';
    classroom.link = '';
    intent = 'teacher';
    if (!startClassHosting(name, options)) {
      intent = '';
      classroom.error = p2p.error || 'Could not start the class.';
      return;
    }
    const session = crypto.randomUUID().replace(/-/g, '').slice(0, 20);
    const config = mailboxConfig();
    openClassLink(session, box);
    classroom.link = liveLink(shareBase(), session, config?.source === 'local' ? config : undefined);
    classroom.appLink = desktopLink(session, config?.source === 'local' ? config : undefined);
    return;
  }
  live.made = ''; // the link of an earlier class is not this one's
  join(name, '', { mode: 'class' }); // (joining first leaves any room this device was in, which clears the class state)
  classroom.transport = 'relay';
  intent = 'teacher';
  classroom.busy = true;
}

/** A student joins the class a link named. */
export function joinClass(studentName: string) {
  const p = classroom.pending;
  if (!p) return;
  if (!studentName.trim()) return void (classroom.error = 'Type your name so the teacher knows who you are.');
  classroom.error = '';
  rename(studentName.trim());
  if (p.live) {
    // A direct class: the mailbox passes the first notes, then this device connects to the teacher's page itself.
    const box: Mailbox | null = p.live.mailbox ? httpMailbox(p.live.mailbox.endpoint, p.live.mailbox.key) : configuredMailbox();
    if (!box) return void (classroom.error = 'This copy of CaveCAD has no class mailbox, so it cannot follow this link. Ask your teacher for a link made from this site.');
    classroom.busy = true;
    void joinViaLink(p.live.session, box).then((problem) => {
      if (problem) {
        classroom.busy = false;
        classroom.error = problem;
        leave();
      }
    });
    return;
  }
  join(p.room, p.invite, { joinOnly: true });
  intent = 'student';
  classroom.busy = true;
}

/** Gives up on a class that cannot be started or joined. Returns false: the welcome is refused. */
function fail(why: string): false {
  leave();
  classroom.error = why;
  return false;
}

interface ClassWelcome {
  role: 'teacher' | 'student';
  private?: boolean;
  name: string;
  started: number;
  ended: boolean;
  chat: ChatEntry[];
  hands: Hand[];
  mirror: string;
}

setRoomHooks({
  welcome: (m) => {
    const c = m.class as ClassWelcome | undefined;
    // Refusing here stops the cave of some other room replacing this one.
    if (intent === 'teacher' && (!c || c.role !== 'teacher')) return fail(`There is already a room called “${live.room}” on this server. Pick another name for the class.`);
    if (intent === 'student' && (!c || c.role !== 'student')) return fail('That link is not to a class.');
    if (!c) {
      // A link to an ordinary shared session (not a class): connected, so the join page is done.
      if (classroom.pending?.live) {
        classroom.pending = null;
        classroom.busy = false;
      }
      return;
    }
    if (intent === '') classroom.transport = p2p.role !== 'none' ? 'p2p' : 'relay'; // a class joined with codes, not a link
    classroom.role = c.role;
    classroom.name = c.name;
    classroom.started = c.started;
    classroom.ended = c.ended;
    classroom.chat = mergeEntries([], c.chat ?? []);
    classroom.hands = c.hands ?? [];
    classroom.unread = 0;
    classroom.busy = false;
    classroom.pending = null;
    classroom.dialog = false;
    classroom.error = '';
    if (c.role === 'student') {
      // Only watching: the engine refuses writes, the page ignores the mouse and keys, and the teacher's view is put on.
      readOnly.on = true;
      classroom.private = c.private !== false; // a class from an older build is private too
      readOnly.noKeep = classroom.private;
      cave.hasCave = true;
      cave.name = c.name;
      shelf.open = false;
      cave.newDialog = false;
      blockInput();
      let state: unknown = null;
      try {
        state = c.mirror ? JSON.parse(c.mirror) : null;
      } catch {
        /* an unreadable view: the next one will do */
      }
      if (state) setTimeout(() => applyMirror(state), 300); // after the cave has arrived
    } else {
      // The teacher: make the link students will open, and start sharing the view.
      if (classroom.transport === 'relay') setTimeout(() => createInvite(c.name, 12, 0), 0);
      startMirroring();
    }
  },
  message: (m) => {
    if (m.t === 'chat') {
      const e = m.entry as ChatEntry;
      const fresh = !classroom.chat.some((x) => x.n === e.n);
      classroom.chat = mergeEntries(classroom.chat, [e]);
      if (fresh) attend(e);
      if (!classroom.drawer && e.kind === 'say' && e.id !== live.me) classroom.unread += 1;
    } else if (m.t === 'hands') {
      classroom.hands = (m.hands as Hand[]) ?? [];
      // a hand that has come down needs no card any more
      classroom.alerts = classroom.alerts.filter((a) => a.kind !== 'hand' || classroom.hands.some((h) => h.id === a.whoId));
      if (!classroom.hands.length) stopFlash();
    } else if (m.t === 'mirror') {
      if (classroom.role === 'student') applyMirror(m.state);
    } else if (m.t === 'ended') {
      classroom.ended = true;
      stopMirroring();
      if (classroom.role === 'student' && classroom.private) {
        classroom.notice = 'The class has ended. The cave will be cleared from this computer in a few seconds.';
        setTimeout(() => classroom.role === 'student' && classroom.ended && leaveClass(), 8000);
      }
    } else if (m.t === 'denied') {
      notify(String(m.message ?? ''));
    } else if (m.t === 'log') {
      save(transcript(String(m.name ?? classroom.name), Number(m.started ?? classroom.started), (m.entries as ChatEntry[]) ?? [], teacherName()), transcriptName(classroom.name, classroom.started));
    }
  },
  left: () => {
    const wasStudent = classroom.role === 'student';
    stopMirroring();
    unblock?.();
    unblock = null;
    readOnly.on = false;
    const clear = wasStudent && readOnly.noKeep;
    readOnly.noKeep = false;
    forgetMirror();
    if (clear) wipeStudentCave();
    intent = '';
    classroom.role = 'none';
    classroom.ended = false;
    classroom.chat = [];
    classroom.hands = [];
    classroom.unread = 0;
    classroom.busy = false;
    classroom.link = '';
  },
});

function notify(text: string) {
  classroom.notice = text;
  clearTimeout(noticeTimer);
  noticeTimer = setTimeout(() => (classroom.notice = ''), 4000);
}

const teacherName = (): string => live.people.find((p) => p.role === 'teacher')?.name ?? '';

// ---- what people do in a class -------------------------------------------------------------------

/** Says a line to the class (a question, an answer). */
export function say(text: string): boolean {
  const t = text.trim();
  return t ? transmit({ t: 'say', text: t }) : false;
}

/** A student raises or lowers their hand. */
export function raiseHand(up: boolean) {
  transmit({ t: 'hand', up });
}

/** The teacher calls on a student: their hand goes down. */
export function callOn(id: string) {
  transmit({ t: 'hand', up: false, who: id });
}

/** The teacher ends the class: nothing more is said, and the record is complete. */
export function endClass() {
  transmit({ t: 'end' });
  stopMirroring();
}

/** The teacher asks for the transcript, which is then saved as a text file. */
export function downloadLog() {
  transmit({ t: 'getlog' });
}

/** Leaves the class (a student stops watching; a teacher closes the room on this device). The cave stays as it is. */
export function leaveClass() {
  leave();
}

/**
 * A student's page held the teacher's cave only to look at it. When they leave (or the class ends, or the connection drops) it is
 * all let go: the cave, its scan pages, what was on show and where the teacher was looking, so nothing of it stays on this
 * computer. What is left is a clean page, as at the start.
 */
function wipeStudentCave() {
  try {
    engine.syncLoad('[]'); // an empty cave in place of the teacher's
  } catch {
    /* the engine was not running: nothing is held */
  }
  clearScans();
  cave.hasCave = false;
  cave.name = '';
  cave.fileName = '';
  cave.trip = '';
  cave.run = '';
  cave.station = '';
  cave.selected = '';
  cave.tool = '';
  cave.pick = { kind: '', id: '' };
  filter.hiddenTrips = [];
  shelf.currentId = '';
  cave.opened += 1; // every view reads the empty cave
  changed();
}

function save(text: string, fileName: string) {
  const url = URL.createObjectURL(new Blob([text], { type: 'text/plain' }));
  const a = document.createElement('a');
  a.href = url;
  a.download = fileName;
  a.click();
  URL.revokeObjectURL(url);
}

// ---- the teacher's view goes to the students -----------------------------------------------------

/** What this page says about itself in the mirror: what is on show and what is picked, none of it the cave. */
registerMirror('cave', {
  read: () => ({
    trip: cave.trip,
    run: cave.run,
    station: cave.station,
    tool: cave.tool,
    selected: cave.selected,
    pick: { kind: cave.pick.kind, id: cave.pick.id },
    adjusted: cave.adjusted,
    units: cave.units,
    sections: { ...cave.showSections },
    hidden: { types: [...filter.hiddenTypes], trips: [...filter.hiddenTrips] },
    options: { ...cave.options },
  }),
  write: (state) => {
    const s = state as Record<string, any>;
    // Only what differs is touched: setting a value to what it already is would still wake everything that watches it,
    // and re-reading the cave for a change of trip or pointer is work nobody needs.
    const set = <K extends keyof typeof cave>(key: K, value: (typeof cave)[K]) => {
      if (cave[key] !== value) cave[key] = value;
    };
    set('trip', String(s.trip ?? ''));
    set('run', String(s.run ?? ''));
    set('station', String(s.station ?? ''));
    set('tool', String(s.tool ?? ''));
    set('selected', String(s.selected ?? ''));
    if (cave.pick.kind !== (s.pick?.kind ?? '') || cave.pick.id !== (s.pick?.id ?? '')) cave.pick = { kind: s.pick?.kind ?? '', id: s.pick?.id ?? '' };
    let reread = false;
    const adjusted = s.adjusted !== false;
    if (cave.adjusted !== adjusted) {
      cave.adjusted = adjusted;
      reread = true;
    }
    if ((s.units === 'imperial' || s.units === 'metric') && cave.units !== s.units) cave.units = s.units;
    if (s.sections && JSON.stringify(s.sections) !== JSON.stringify({ ...cave.showSections, ...s.sections })) {
      cave.showSections = { ...cave.showSections, ...s.sections };
      reread = true;
    }
    if (s.hidden) {
      const types = [...(s.hidden.types ?? [])];
      const trips = [...(s.hidden.trips ?? [])];
      if (JSON.stringify(types) !== JSON.stringify(filter.hiddenTypes)) {
        filter.hiddenTypes = types;
        reread = true;
      }
      if (JSON.stringify(trips) !== JSON.stringify(filter.hiddenTrips)) {
        filter.hiddenTrips = trips;
        reread = true;
      }
    }
    if (s.options && JSON.stringify(s.options) !== JSON.stringify(cave.options)) cave.options = { ...s.options };
    if (reread) changed();
  },
});

/** What was last sent for each part, so only a part that changed goes out (a camera move is a few numbers, not the whole page). */
const sentParts = new Map<string, string>();

function startMirroring() {
  stopMirroring();
  sentParts.clear();
  lastSent = '';
  beat = setInterval(() => {
    if (classroom.role !== 'teacher' || classroom.ended || live.status !== 'live') return;
    const state = captureMirror();
    const delta: Record<string, unknown> = {};
    const sent: [string, string][] = [];
    for (const [key, value] of Object.entries(state)) {
      const json = JSON.stringify(value);
      if (sentParts.get(key) === json) continue;
      delta[key] = value;
      sent.push([key, json]);
    }
    if (!sent.length) return;
    if (transmit({ t: 'mirror', state: delta })) for (const [key, json] of sent) sentParts.set(key, json);
  }, 120);
}

function stopMirroring() {
  clearInterval(beat);
  beat = undefined;
}

// ---- a student only watches ----------------------------------------------------------------------

const BLOCKED = ['pointerdown', 'pointerup', 'mousedown', 'mouseup', 'click', 'dblclick', 'auxclick', 'contextmenu', 'wheel', 'keydown', 'keyup', 'keypress', 'touchstart', 'touchend', 'input', 'beforeinput', 'change', 'paste', 'cut', 'dragstart', 'drop'];

/** Keeps every click, key and wheel turn from reaching the page, except in the class's own controls. Pointer
 *  movement is left alone, so the views can report where the student's pointer is. */
function blockInput() {
  unblock?.();
  const stop = (e: Event) => {
    const t = e.target as Element | null;
    if (t && t.closest?.('[data-classroom-ui]')) return;
    e.stopImmediatePropagation();
    // Browser shortcuts (reload, copy, zoom) still work; they just do not reach the page's own handlers.
    if (e instanceof KeyboardEvent && (e.ctrlKey || e.metaKey) && !['z', 'y', 's', 'o', 'n', 'x', 'v'].includes(e.key.toLowerCase())) return;
    e.preventDefault();
  };
  for (const type of BLOCKED) window.addEventListener(type, stop, { capture: true, passive: false });
  unblock = () => {
    for (const type of BLOCKED) window.removeEventListener(type, stop, { capture: true });
  };
}

/** Whether this device is only watching (the views use it to leave the teacher's view alone). */
export const watching = (): boolean => classroom.role === 'student';

/** A student's cave is the teacher's: it is never kept in this browser. */
export function keepsCave(): boolean {
  return classroom.role !== 'student';
}

// Dev-only handle for tests and debugging in the browser console.
if (import.meta.env.DEV) (window as unknown as { __classroom: unknown }).__classroom = { classroom, startClass, joinClass, say, raiseHand };
