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
import { cave, changed, filter } from './cave.svelte';
import { applyMirror, captureMirror, forgetMirror, registerMirror } from './classroom-mirror';
import { classLink, mergeEntries, readClassLink, transcript, transcriptName, type ChatEntry, type Hand } from './classroom-log';
import { readOnly } from './readonly';
import { shelf } from './shelf.svelte';
import { createInvite, join, leave, live, rename, sendRoom, setRoomHooks } from './sync.svelte';

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
  chat: [] as ChatEntry[],
  /** Hands up now, longest waiting first. */
  hands: [] as Hand[],
  /** Lines said while the drawer was shut (for the badge). */
  unread: 0,
  /** The side drawer (chat, hands, people) is open. */
  drawer: true,
  /** A short notice from the relay ("Slow down a little."), shown for a moment. */
  notice: '',
  /** A class link this page was opened with, waiting for the student to give a name and join. */
  pending: null as { room: string; invite: string } | null,
  /** The Start a Classroom dialog is open. */
  dialog: false,
  /** Why starting or joining failed, in words. */
  error: '',
  /** Connecting now (the Join or Start button was pressed). */
  busy: false,
});

/** Whether this device's hand is up. */
export const handIsUp = (): boolean => classroom.hands.some((h) => h.id === live.me);

/** The link students open, once the teacher has an invite. Empty until then. */
export function studentLink(): string {
  return live.made && classroom.name ? classLink(location.href, classroom.name, live.made) : '';
}

/** Reads a class link the page was opened with (and tidies the address bar). */
export function readClassLinkFromUrl() {
  const found = readClassLink(location.search);
  if (!found) return;
  classroom.pending = found;
  history.replaceState(null, '', location.pathname + location.hash);
}

let intent: 'teacher' | 'student' | '' = '';
let noticeTimer: ReturnType<typeof setTimeout> | undefined;
let beat: ReturnType<typeof setInterval> | undefined;
let lastSent = '';
let unblock: (() => void) | null = null;

// ---- starting and joining ------------------------------------------------------------------------

/** The teacher starts a class from the open cave. */
export function startClass(className: string, teacherName: string) {
  const name = className.trim();
  if (!name) return void (classroom.error = 'Give the class a name.');
  if (!cave.hasCave) return void (classroom.error = 'Open or start a cave first: there is nothing to teach from yet.');
  classroom.error = '';
  if (teacherName.trim()) rename(teacherName.trim());
  live.made = ''; // the link of an earlier class is not this one's
  join(name, '', { mode: 'class' }); // (joining first leaves any room this device was in, which clears the class state)
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
    if (!c) return;
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
      setTimeout(() => createInvite(c.name, 12, 0), 0);
      startMirroring();
    }
  },
  message: (m) => {
    if (m.t === 'chat') {
      const e = m.entry as ChatEntry;
      classroom.chat = mergeEntries(classroom.chat, [e]);
      if (!classroom.drawer && e.kind === 'say' && e.id !== live.me) classroom.unread += 1;
    } else if (m.t === 'hands') {
      classroom.hands = (m.hands as Hand[]) ?? [];
    } else if (m.t === 'mirror') {
      if (classroom.role === 'student') applyMirror(m.state);
    } else if (m.t === 'ended') {
      classroom.ended = true;
      stopMirroring();
    } else if (m.t === 'denied') {
      notify(String(m.message ?? ''));
    } else if (m.t === 'log') {
      save(transcript(String(m.name ?? classroom.name), Number(m.started ?? classroom.started), (m.entries as ChatEntry[]) ?? [], teacherName()), transcriptName(classroom.name, classroom.started));
    }
  },
  left: () => {
    stopMirroring();
    unblock?.();
    unblock = null;
    readOnly.on = false;
    forgetMirror();
    intent = '';
    classroom.role = 'none';
    classroom.ended = false;
    classroom.chat = [];
    classroom.hands = [];
    classroom.unread = 0;
    classroom.busy = false;
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
  return t ? sendRoom({ t: 'say', text: t }) : false;
}

/** A student raises or lowers their hand. */
export function raiseHand(up: boolean) {
  sendRoom({ t: 'hand', up });
}

/** The teacher calls on a student: their hand goes down. */
export function callOn(id: string) {
  sendRoom({ t: 'hand', up: false, who: id });
}

/** The teacher ends the class: nothing more is said, and the record is complete. */
export function endClass() {
  sendRoom({ t: 'end' });
  stopMirroring();
}

/** The teacher asks for the transcript, which is then saved as a text file. */
export function downloadLog() {
  sendRoom({ t: 'getlog' });
}

/** Leaves the class (a student stops watching; a teacher closes the room on this device). The cave stays as it is. */
export function leaveClass() {
  leave();
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
    cave.trip = String(s.trip ?? '');
    cave.run = String(s.run ?? '');
    cave.station = String(s.station ?? '');
    cave.tool = String(s.tool ?? '');
    cave.selected = String(s.selected ?? '');
    cave.pick = { kind: s.pick?.kind ?? '', id: s.pick?.id ?? '' };
    cave.adjusted = s.adjusted !== false;
    if (s.units === 'imperial' || s.units === 'metric') cave.units = s.units;
    if (s.sections) cave.showSections = { ...cave.showSections, ...s.sections };
    if (s.hidden) {
      filter.hiddenTypes = [...(s.hidden.types ?? [])];
      filter.hiddenTrips = [...(s.hidden.trips ?? [])];
    }
    if (s.options) cave.options = { ...s.options };
    changed();
  },
});

function startMirroring() {
  stopMirroring();
  lastSent = '';
  beat = setInterval(() => {
    if (classroom.role !== 'teacher' || classroom.ended || live.status !== 'live') return;
    const state = captureMirror();
    const key = JSON.stringify(state);
    if (key === lastSent) return;
    if (sendRoom({ t: 'mirror', state })) lastSent = key;
  }, 150);
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
