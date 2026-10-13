// Working together with no server: devices connect straight to each other (WebRTC data channels).
// Two short codes are swapped by message to introduce them (see `p2p-code.ts`); after that edits, pointers,
// where the host is looking, and scan pages travel directly between the devices.
//
//   HOST   has the cave. Makes an invite code per person; each guest's reply code is pasted back; the
//          host lets them in (or not) and then passes everyone's edits along to everyone else.
//   GUEST  pastes an invite code, gets a reply code to send back, and when the host pastes it the
//          host's cave arrives and from then on edits go both ways.
//
// The cave's edits merge in any order, so there is no referee: the host is only the hub that forwards.
// If the host goes, guests keep their copy, and can join the host again later with new codes to merge.
import { relayServers } from './p2p-relay.svelte';
import { engine } from './engine';
import { cave, changed, markSaved } from './cave.svelte';
import { decodeCode, encodeCode } from './p2p-code';
import { Reassembler, frames } from './p2p-wire';
import { addRemoteFile, fileOf, have, scans, select } from './scans.svelte';
import { fadeCursors, live, me, myPersonId, roomEvents, setPeerHooks, type Person } from './sync.svelte';
import { hostLoop, joinLoop } from './signal-handshake';
import type { Mailbox } from './signal';
import { CLASS_MESSAGES, classLeft, classMessage, classWelcome, newClass, type ClassState, type RoomIo } from './classroom-room.mjs';
import { flush as keepShelf, shelf } from './shelf.svelte';

/** A person the host has made an invite for. */
export interface PeerInvite {
  id: string;
  /** The text to send them. */
  code: string;
  state: 'waiting' | 'connecting' | 'asking' | 'connected' | 'closed';
  /** Their name once they have said hello. */
  name: string;
}

export const p2p = $state({
  role: 'none' as 'none' | 'host' | 'guest',
  /** Look up this device's public address with a public STUN service, so devices on different networks can find each other. */
  stun: true,
  /** The host lets people in without asking. */
  auto: false,
  invites: [] as PeerInvite[],
  /** Why the last step failed, in plain words. */
  error: '',
  /** The guest's reply code, to send back to the host. */
  reply: '',
  /** A guest follows what the host is looking at (trip, run, station, scan). */
  following: true,
  /** The Live dialog is open. */
  dialog: false,
  /** What the guest is waiting for, in words. */
  waiting: '',
});

const SEND_LOW = 256_000;
const SEND_HIGH = 1_000_000;
const FLUSH_MS = 80;
const GATHER_MS = 5000;
const CHUNK = 16_000;

interface Peer {
  id: string;
  /** The scan pages this peer says it has (so a missing page can be asked for from someone who holds it). */
  has?: Set<string>;
  pc: RTCPeerConnection;
  ch: RTCDataChannel | null;
  /** The person this peer is, once admitted. */
  person: Person | null;
  name: string;
  color: string;
  reasm: Reassembler;
  /** Sends in the order given, whatever their size. */
  tail: Promise<void>;
  seq: number;
  /** A scan page arriving from the other side. */
  incoming: { path: string; type: string; size: number; parts: ArrayBuffer[]; got: number } | null;
}

const peers = new Map<string, Peer>();
/** The class this page is teaching, when it hosts one: students are the guests, who only watch (see `classroom.svelte.ts`). */
let cls: ClassState | null = null;
let guestPeer: Peer | null = null;
let timer: ReturnType<typeof setInterval> | undefined;
let sessionId = '';
let lastHost = '';
/** How long a joiner waits for the host to use its reply code before it gives up. */
const JOIN_PATIENCE_MS = 5 * 60_000;
const wanted = new Set<string>();
/** When each page was asked for, so a request that went nowhere is asked again. */
const wantedAt = new Map<string, number>();
/** Pages a guest asked the host for that the host is fetching from another guest first. */
const relayTo = new Map<string, Set<Peer>>();
/** What this device last said it holds. */
let advertised = '';
let scanTimer: ReturnType<typeof setInterval> | undefined;

const rid = () => Math.random().toString(36).slice(2, 14).padEnd(12, 'x');
const STUN: RTCIceServer = { urls: ['stun:stun.l.google.com:19302', 'stun:stun1.l.google.com:19302'] };
/** The public-address lookup plus the person's relay (if any), which only carries data when a direct path fails. */
const iceServers = async (): Promise<RTCIceServer[]> => (p2p.stun ? [STUN, ...(await relayServers())] : []);

// ---- sending -------------------------------------------------------------------------------------

function drain(ch: RTCDataChannel): Promise<void> {
  return new Promise((resolve) => {
    ch.bufferedAmountLowThreshold = SEND_LOW;
    ch.onbufferedamountlow = () => {
      ch.onbufferedamountlow = null;
      resolve();
    };
  });
}

/** Queues a message to a peer (cut into frames, in order with everything else sent to it). */
function send(p: Peer, msg: object) {
  const text = JSON.stringify(msg);
  p.tail = p.tail
    .then(async () => {
      const ch = p.ch;
      if (!ch || ch.readyState !== 'open') return;
      for (const f of frames(text, ++p.seq)) {
        if (ch.bufferedAmount > SEND_HIGH) await drain(ch);
        ch.send(f);
      }
    })
    .catch(() => undefined);
}

/** Queues a page's bytes to a peer, between a header and a trailer, in order. */
function sendFile(p: Peer, path: string, file: File) {
  send(p, { t: 'scan', path, type: file.type, size: file.size });
  p.tail = p.tail
    .then(async () => {
      const ch = p.ch;
      if (!ch || ch.readyState !== 'open') return;
      const bytes = await file.arrayBuffer();
      for (let at = 0; at < bytes.byteLength; at += CHUNK) {
        if (ch.bufferedAmount > SEND_HIGH) await drain(ch);
        ch.send(bytes.slice(at, at + CHUNK));
      }
    })
    .catch(() => undefined);
  send(p, { t: 'scanEnd', path });
}

const admitted = () => [...peers.values()].filter((p) => p.person && p.ch?.readyState === 'open');

function toAll(msg: object, except?: Peer) {
  for (const p of admitted()) if (p !== except) send(p, msg);
}

// ---- connections ---------------------------------------------------------------------------------

function gathered(pc: RTCPeerConnection): Promise<void> {
  return new Promise((resolve) => {
    if (pc.iceGatheringState === 'complete') return resolve();
    const done = () => {
      pc.removeEventListener('icegatheringstatechange', check);
      clearTimeout(t);
      resolve();
    };
    const check = () => pc.iceGatheringState === 'complete' && done();
    const t = setTimeout(done, GATHER_MS); // whatever was found by now is enough to try
    pc.addEventListener('icegatheringstatechange', check);
  });
}

async function newPeer(id: string): Promise<Peer> {
  const pc = new RTCPeerConnection({ iceServers: await iceServers() });
  const peer: Peer = { id, pc, ch: null, person: null, name: '', color: '', reasm: new Reassembler(), tail: Promise.resolve(), seq: 0, incoming: null };
  peers.set(id, peer);
  return peer;
}

function wire(peer: Peer, ch: RTCDataChannel, role: 'host' | 'guest') {
  peer.ch = ch;
  ch.binaryType = 'arraybuffer';
  ch.onopen = () => {
    if (role === 'guest') {
      const rejoin = !!lastHost && lastHost === sessionId && cave.hasCave;
      send(peer, { t: 'hello', id: myPersonId, name: me.name.trim() || 'Someone', color: me.color, ...(rejoin ? { rejoin: true, ops: JSON.parse(engine.syncSnapshot()) } : {}) });
      p2p.waiting = 'Connected. Waiting for the host to let you in…';
    }
    const inv = p2p.invites.find((i) => i.id === peer.id);
    if (inv && inv.state === 'connecting') inv.state = 'connected';
  };
  ch.onmessage = (ev) => {
    if (typeof ev.data === 'string') {
      const whole = peer.reasm.push(ev.data);
      if (whole === null) return;
      try {
        handle(peer, JSON.parse(whole), role);
      } catch {
        /* a message that does not parse is ignored */
      }
    } else if (peer.incoming) {
      peer.incoming.parts.push(ev.data as ArrayBuffer);
      peer.incoming.got += (ev.data as ArrayBuffer).byteLength;
    }
  };
  ch.onclose = () => closed(peer, role);
}

function closed(peer: Peer, role: 'host' | 'guest') {
  if (!peers.has(peer.id)) return;
  peers.delete(peer.id);
  try {
    peer.pc.close();
  } catch {
    /* already closed */
  }
  if (role === 'host') {
    const inv = p2p.invites.find((i) => i.id === peer.id);
    if (inv) inv.state = 'closed';
    if (peer.person) {
      if (cls) classLeft(cls, peer.person.id, classIo(peer));
      announce();
      engine.log('info', 'share', `${peer.name || 'Someone'} left the session`);
    }
  } else if (guestPeer === peer) {
    guestPeer = null;
    stopTimer();
    live.status = 'off';
    live.people = [];
    live.cursors = {};
    p2p.role = 'none';
    p2p.reply = '';
    p2p.waiting = '';
    roomEvents.left(); // a class with no teacher any more: this device may look around on its own
    p2p.error = 'The host’s connection ended. Your copy of the cave is kept here; join again with new codes and the two copies merge.';
  }
}

function announce() {
  const list: Person[] = [{ id: myPersonId, name: me.name.trim() || 'Host', color: me.color, ...(cls ? { role: 'teacher' as const } : {}) }, ...admitted().map((p) => (cls ? { ...p.person!, role: 'student' as const } : p.person!))];
  live.people = list;
  toAll({ t: 'peers', people: list, owner: myPersonId });
}

// ---- what arrives --------------------------------------------------------------------------------

type Msg = {
  t: string;
  id?: string;
  name?: string;
  color?: string;
  rejoin?: boolean;
  ops?: unknown[];
  log?: unknown[];
  merge?: boolean;
  caveName?: string;
  people?: Person[];
  owner?: string;
  path?: string;
  paths?: string[];
  type?: string;
  size?: number;
  view?: string;
  ctx?: string;
  x?: number;
  y?: number;
  tool?: string;
  trip?: string;
  run?: string;
  station?: string;
  scan?: string;
};

function handle(peer: Peer, m: Msg, role: 'host' | 'guest') {
  if (role === 'host') hostHears(peer, m);
  else guestHears(peer, m);
  // Both sides: pages
  if (m.t === 'has' && Array.isArray(m.paths)) {
    peer.has = new Set(m.paths.map(String));
  } else if (m.t === 'want' && m.path) {
    const f = fileOf(m.path);
    if (f) sendFile(peer, m.path, f);
    else if (role === 'host') {
      // the host does not have it, but another guest may: ask them and pass it on when it arrives
      const holder = admitted().find((q) => q !== peer && q.has?.has(m.path!));
      if (holder) {
        const to = relayTo.get(m.path) ?? new Set<Peer>();
        to.add(peer);
        relayTo.set(m.path, to);
        if (!wanted.has(m.path)) {
          wanted.add(m.path);
          wantedAt.set(m.path, Date.now());
          send(holder, { t: 'want', path: m.path });
        }
      }
    }
  } else if (m.t === 'scan' && m.path) {
    peer.incoming = { path: m.path, type: m.type ?? '', size: m.size ?? 0, parts: [], got: 0 };
  } else if (m.t === 'scanEnd' && peer.incoming && peer.incoming.path === m.path) {
    const inc = peer.incoming;
    peer.incoming = null;
    const name = inc.path.slice(inc.path.lastIndexOf('/') + 1);
    addRemoteFile(inc.path, new File(inc.parts, name, { type: inc.type }));
    wanted.delete(inc.path);
    wantedAt.delete(inc.path);
    const file = fileOf(inc.path);
    if (file) for (const q of relayTo.get(inc.path) ?? []) if (q.ch?.readyState === 'open') sendFile(q, inc.path, file);
    relayTo.delete(inc.path);
    if (p2p.following && lastView.scan === inc.path) select(inc.path);
  }
}

function hostHears(peer: Peer, m: Msg) {
  if (m.t === 'hello') {
    peer.name = String(m.name ?? 'Someone').slice(0, 40);
    peer.color = String(m.color ?? '#888888').slice(0, 20);
    const inv = p2p.invites.find((i) => i.id === peer.id);
    if (inv) inv.name = peer.name;
    const rejoin = m.rejoin === true && Array.isArray(m.ops) ? m.ops : null;
    const admit = () => {
      peer.person = { id: String(m.id ?? peer.id).slice(0, 40), name: peer.name, color: peer.color };
      if (inv) inv.state = 'connected';
      if (rejoin && !cls) {
        engine.syncApply(JSON.stringify(rejoin)); // what that guest made while away
        changed();
      }
      send(peer, { t: 'welcome', merge: !!rejoin, log: JSON.parse(engine.syncSnapshot()), caveName: cave.name, host: { id: myPersonId, name: me.name, color: me.color }, ...(cls ? { class: classWelcome(cls, 'student') } : {}) });
      announce();
      sendView(peer);
      engine.log('info', 'share', `${peer.name} joined the session`);
    };
    if (p2p.auto) admit();
    else {
      if (inv) inv.state = 'asking';
      pending.set(peer.id, admit);
    }
  } else if (!peer.person) {
    return; // nothing is taken from someone who has not been let in
  } else if (cls && m.t === 'ops') {
    send(peer, { t: 'denied', what: 'edit', message: 'Only the teacher can change the cave in a class.' }); // students only watch
  } else if (cls && CLASS_MESSAGES.includes(m.t)) {
    classMessage(cls, { id: peer.person.id, name: peer.person.name, role: 'student' }, m as { t: string }, classIo(peer));
  } else if (cls && m.t === 'presence') {
    // What a student does is for the teacher to see, not for the other students.
    live.cursors[peer.person.id] = { id: peer.person.id, view: String(m.view ?? ''), ctx: String(m.ctx ?? ''), x: Number(m.x), y: Number(m.y), tool: String(m.tool ?? ''), seen: Date.now() };
  } else if (m.t === 'ops' && Array.isArray(m.ops)) {
    engine.syncApply(JSON.stringify(m.ops));
    changed();
    toAll({ t: 'ops', ops: m.ops }, peer); // on to everyone else
  } else if (m.t === 'presence') {
    const msg = { t: 'presence', id: peer.person.id, view: m.view, ctx: m.ctx, x: m.x, y: m.y, tool: m.tool };
    live.cursors[peer.person.id] = { id: peer.person.id, view: String(m.view ?? ''), ctx: String(m.ctx ?? ''), x: Number(m.x), y: Number(m.y), tool: String(m.tool ?? ''), seen: Date.now() };
    toAll(msg, peer);
  }
}

function guestHears(peer: Peer, m: Msg) {
  if (m.t === 'welcome') {
    // A class the host is teaching: this device only watches (the class refuses a welcome it does not want).
    if (roomEvents.welcome(m as unknown as Record<string, unknown>) === false) {
      leaveP2p();
      return;
    }
    if (m.merge && !(m as { class?: unknown }).class) {
      engine.syncApply(JSON.stringify(m.log ?? []));
    } else {
      const r = engine.syncLoad(JSON.stringify(m.log ?? []));
      if (r !== 'ok') {
        p2p.error = r.replace(/^error: /, '');
        leaveP2p();
        return;
      }
      void keepShelf(); // what was open here is kept before the host's cave replaces it
      shelf.currentId = ''; // the host's cave is not one of this device's own
      cave.hasCave = true;
      cave.name = String(m.caveName ?? 'Shared cave');
      cave.fileName = `${cave.name}.dxf`;
      cave.trip = '';
      cave.run = '';
      cave.station = '';
      cave.opened += 1;
      markSaved();
    }
    lastHost = sessionId;
    live.status = 'live';
    live.room = 'peer session';
    live.owner = false;
    live.ownerId = (m as { host?: { id: string } }).host?.id ?? '';
    live.error = '';
    p2p.waiting = '';
    p2p.error = '';
    p2p.dialog = false;
    shelf.open = false;
    changed();
    startTimer();
  } else if (m.t === 'peers') {
    live.people = m.people ?? [];
    live.ownerId = m.owner ?? '';
    const here = new Set(live.people.map((p) => p.id));
    for (const id of Object.keys(live.cursors)) if (!here.has(id)) delete live.cursors[id];
  } else if (m.t === 'ops' && Array.isArray(m.ops)) {
    engine.syncApply(JSON.stringify(m.ops));
    changed();
  } else if (m.t === 'presence' && m.id && m.id !== myPersonId) {
    live.cursors[m.id] = { id: m.id, view: String(m.view ?? ''), ctx: String(m.ctx ?? ''), x: Number(m.x), y: Number(m.y), tool: String(m.tool ?? ''), seen: Date.now() };
  } else if (m.t === 'chat' || m.t === 'hands' || m.t === 'mirror' || m.t === 'ended' || m.t === 'denied' || m.t === 'log') {
    roomEvents.message(m as unknown as Record<string, unknown>);
  } else if (m.t === 'view') {
    followHost(m);
  } else if (m.t === 'refused') {
    p2p.error = 'The host did not let you in.';
    leaveP2p();
  }
}

// ---- following the host --------------------------------------------------------------------------

const lastView = { trip: '', run: '', station: '', scan: '' };

function currentView() {
  return { t: 'view', trip: cave.trip, run: cave.run, station: cave.station, scan: scans.selected };
}

function sendView(peer: Peer) {
  send(peer, currentView());
}

let viewTimer: ReturnType<typeof setTimeout> | undefined;

/** The host tells guests where it is looking (call when the trip, run, station or scan on show changes). */
export function viewChanged() {
  if (p2p.role !== 'host') return;
  clearTimeout(viewTimer);
  viewTimer = setTimeout(() => toAll(currentView()), 120);
}

function followHost(m: Msg) {
  lastView.trip = m.trip ?? '';
  lastView.run = m.run ?? '';
  lastView.station = m.station ?? '';
  lastView.scan = m.scan ?? '';
  if (!p2p.following) return;
  applyView();
}

/** Puts this device on what the host is looking at. */
export function applyView() {
  if (lastView.trip && engine.notebookTrips().some((t) => t.id === lastView.trip)) cave.trip = lastView.trip;
  if (lastView.run) cave.run = lastView.run;
  if (lastView.station) cave.station = lastView.station;
  const path = lastView.scan;
  if (!path) return;
  if (have(path)) {
    if (scans.selected !== path) select(path);
  } else if (!wanted.has(path) && guestPeer) {
    wanted.add(path); // not here: ask the host for the page itself
    send(guestPeer, { t: 'want', path });
  }
}

// ---- the 80 ms beat ------------------------------------------------------------------------------

function startTimer() {
  stopTimer();
  scanTimer = setInterval(scanBeat, 2500);
  timer = setInterval(() => {
    fadeCursors();
    const ops = engine.syncOutbox();
    if (ops === '[]') return;
    const parsed = JSON.parse(ops);
    if (p2p.role === 'host') toAll({ t: 'ops', ops: parsed });
    else if (guestPeer) send(guestPeer, { t: 'ops', ops: parsed });
  }, FLUSH_MS);
}

function stopTimer() {
  clearInterval(timer);
  timer = undefined;
  clearInterval(scanTimer);
  scanTimer = undefined;
  advertised = '';
  wantedAt.clear();
  relayTo.clear();
}

/**
 * Keeps everyone's scan pages in step: each device says which pages of the cave it holds, and asks for
 * the pages it is missing from someone who holds them (guests ask the host, which fetches from
 * another guest when it does not have the page itself). Checked every few seconds, so a page linked
 * or opened later travels too.
 */
function scanBeat() {
  if (p2p.role === 'none') return;
  const linked = [...new Set(engine.scanLinks().map((l) => l.path))];
  const mine = linked.filter(have).sort();
  const key = mine.join('\n');
  if (key !== advertised) {
    advertised = key;
    const msg = { t: 'has', paths: mine };
    if (p2p.role === 'host') toAll(msg);
    else if (guestPeer) send(guestPeer, msg);
  }
  const now = Date.now();
  for (const [path, at] of wantedAt) if (now - at > 30_000) (wanted.delete(path), wantedAt.delete(path)); // asked and nothing came: ask again
  for (const path of linked) {
    if (have(path) || wanted.has(path)) continue;
    const from = p2p.role === 'host' ? admitted().find((q) => q.has?.has(path)) : guestPeer;
    if (!from) continue;
    wanted.add(path);
    wantedAt.set(path, now);
    send(from, { t: 'want', path });
  }
}

setPeerHooks({
  presence: (msg) => {
    if (p2p.role === 'host') toAll({ ...msg, id: myPersonId });
    else if (guestPeer) send(guestPeer, msg);
  },
  leave: () => leaveP2p(),
});

// ---- the host's side -----------------------------------------------------------------------------

const pending = new Map<string, () => void>();

/** Starts sharing the open cave: from now on people can be invited. */
export function startHosting(): boolean {
  if (!cave.hasCave) {
    p2p.error = 'Open or start a cave first: there is nothing to share yet.';
    return false;
  }
  if (typeof RTCPeerConnection === 'undefined') {
    p2p.error = 'This browser cannot connect directly to other devices.';
    return false;
  }
  p2p.error = '';
  p2p.role = 'host';
  sessionId = rid();
  live.status = 'live';
  live.room = 'peer session';
  live.owner = true;
  live.ownerId = myPersonId;
  live.error = '';
  announce();
  engine.syncOutbox(); // everything already made is in the snapshot a guest gets
  startTimer();
  return true;
}

/** How a class's rules reach the people in this session. `from` is the guest a message came from, or null for the teacher's own page. */
function classIo(from: Peer | null): RoomIo {
  return {
    members: () => [{ id: myPersonId, name: me.name.trim() || 'Teacher', role: 'teacher' }, ...admitted().map((p) => ({ id: p.person!.id, name: p.person!.name, role: 'student' }))],
    send: (id, msg) => {
      if (id === myPersonId) roomEvents.message(msg as Record<string, unknown>);
      else {
        const p = admitted().find((q) => q.person!.id === id);
        if (p) send(p, msg);
      }
    },
    all: (msg) => {
      toAll(msg);
      roomEvents.message(msg as Record<string, unknown>);
    },
    others: (msg) => {
      toAll(msg, from ?? undefined);
      if (from) roomEvents.message(msg as Record<string, unknown>);
    },
  };
}

/**
 * Starts sharing the open cave as a class: this page is the teacher, and everyone let in is a student who only
 * watches (their edits are refused here). Students join like any guest, with invite and reply codes, and are let in
 * without asking. Returns false (and says why in `p2p.error`) when it cannot start.
 */
export function startClassHosting(name: string): boolean {
  if (p2p.role !== 'none') {
    p2p.error = 'Stop sharing first: a class starts a session of its own.';
    return false;
  }
  if (!startHosting()) return false;
  cls = newClass(name);
  p2p.auto = true;
  announce();
  return roomEvents.welcome({ class: classWelcome(cls, 'teacher'), owner: true }) !== false;
}

/** Sends a class message: the teacher's page applies it (and passes it on); a student's goes to the teacher. */
export function sendClass(msg: { t: string } & Record<string, unknown>): boolean {
  if (p2p.role === 'host' && cls) {
    classMessage(cls, { id: myPersonId, name: me.name.trim() || 'Teacher', role: 'teacher' }, msg, classIo(null));
    return true;
  }
  if (p2p.role === 'guest' && guestPeer && live.status === 'live') {
    send(guestPeer, msg);
    return true;
  }
  return false;
}

/** Makes an invite: the text to send to one person. */
export async function makeInvite(): Promise<string | null> {
  if (p2p.role !== 'host' && !startHosting()) return null;
  const id = rid();
  const peer = await newPeer(id);
  const ch = peer.pc.createDataChannel('cave', { ordered: true });
  wire(peer, ch, 'host');
  peer.pc.onconnectionstatechange = () => {
    const s = peer.pc.connectionState;
    if (s === 'failed' || s === 'disconnected' || s === 'closed') closed(peer, 'host');
  };
  p2p.invites = [...p2p.invites, { id, code: '', state: 'waiting', name: '' }];
  try {
    await peer.pc.setLocalDescription(await peer.pc.createOffer());
    await gathered(peer.pc);
    const code = await encodeCode({ kind: 'invite', sdp: peer.pc.localDescription!.sdp, sid: id });
    p2p.invites = p2p.invites.map((i) => (i.id === id ? { ...i, code } : i));
  } catch {
    p2p.invites = p2p.invites.filter((i) => i.id !== id);
    peers.delete(id);
    p2p.error = 'Could not make an invite code.';
    return null;
  }
  return id;
}

let stopSignal: (() => void) | null = null;

/**
 * The teacher's page starts letting students in through a class link: anyone who opens the link is given an invite of
 * their own, through the mailbox, and their reply comes back the same way. Needs a class already started
 * (`startClassHosting`). The link is stopped when sharing stops.
 */
export function openClassLink(session: string, box: Mailbox) {
  stopSignal?.();
  stopSignal = hostLoop(box, session, {
    offer: async () => {
      const id = await makeInvite();
      const code = id ? p2p.invites.find((i) => i.id === id)?.code : '';
      return id && code ? { id, code } : null;
    },
    reply: (id, code) => takeReply(id, code),
    trouble: (message) => (p2p.error = message),
  });
}

/** A student's page joins the class a link named, through the mailbox. Gives back a sentence when it cannot. */
export async function joinViaLink(session: string, box: Mailbox): Promise<string | null> {
  return joinLoop(box, session, {
    answer: async (offer) => {
      const problem = await joinWithCode(offer);
      return problem ? { error: problem } : { code: p2p.reply };
    },
  });
}

/** The host pastes a guest's reply code to finish connecting them. */
export async function takeReply(inviteId: string, text: string): Promise<string | null> {
  const peer = peers.get(inviteId);
  if (!peer) return 'That invite is not waiting any more.';
  const code = await decodeCode(text);
  if (!code) return 'That is not a CaveCAD code. Paste the whole reply, starting with CAVE1-.';
  if (code.kind !== 'answer') return 'That is an invite code, not a reply. Paste the reply the guest sent back.';
  if (code.sid !== inviteId) return 'That reply is for a different invite.';
  try {
    await peer.pc.setRemoteDescription({ type: 'answer', sdp: code.sdp });
  } catch {
    return 'That reply could not be used (was it for this invite, and is it all there?).';
  }
  const inv = p2p.invites.find((i) => i.id === inviteId);
  if (inv) inv.state = 'connecting';
  return null;
}

/** Asking: let the person in. */
export function letIn(inviteId: string) {
  pending.get(inviteId)?.();
  pending.delete(inviteId);
}

/** Asking: turn the person away. */
export function turnAway(inviteId: string) {
  pending.delete(inviteId);
  const peer = peers.get(inviteId);
  if (peer) {
    send(peer, { t: 'refused' });
    setTimeout(() => closed(peer, 'host'), 300);
  }
}

/** Removes someone from the session (or withdraws an invite nobody has used). */
export function remove(inviteId: string) {
  const peer = peers.get(inviteId);
  pending.delete(inviteId);
  if (peer) {
    try {
      peer.ch?.close();
    } catch {
      /* already closed */
    }
    closed(peer, 'host');
  }
  p2p.invites = p2p.invites.filter((i) => i.id !== inviteId);
}

// ---- the guest's side ----------------------------------------------------------------------------

/**
 * Joins a session with the host's invite code. Gives back a sentence when the code is no good;
 * otherwise `p2p.reply` fills with the reply code to send back, and the guest waits for the host.
 */
export async function joinWithCode(text: string): Promise<string | null> {
  if (typeof RTCPeerConnection === 'undefined') return 'This browser cannot connect directly to other devices.';
  const code = await decodeCode(text);
  if (!code) return 'That is not a CaveCAD code. Paste the whole invite, starting with CAVE1-.';
  if (code.kind !== 'invite') return 'That is a reply code. Paste the invite the host sent you.';
  leaveP2p();
  p2p.error = '';
  p2p.role = 'guest';
  sessionId = code.sid;
  const peer = await newPeer(code.sid);
  guestPeer = peer;
  peer.pc.ondatachannel = (ev) => wire(peer, ev.channel, 'guest');
  // Until the host pastes the reply there is nobody to answer this device's connection checks, so the
  // browser may report a failure straight away. That is not an answer yet: wait for the host (up to
  // JOIN_PATIENCE_MS), and only a failure after that, or after having connected, ends the attempt.
  let connectedOnce = false;
  let patience: ReturnType<typeof setTimeout> | undefined;
  const giveUp = () => {
    p2p.error = 'Could not connect. The two devices could not find a way to each other (a strict network can block this). Try again, add a relay under \u201cRelay for strict networks\u201d, or try both devices on the same network.';
    leaveP2p();
  };
  peer.pc.onconnectionstatechange = () => {
    const s = peer.pc.connectionState;
    if (s === 'connected') {
      connectedOnce = true;
      clearTimeout(patience);
    } else if (s === 'failed') {
      if (connectedOnce) giveUp();
      else if (!patience) patience = setTimeout(() => (guestPeer === peer && !connectedOnce ? (p2p.error = 'The host did not use your reply code in time. Send it again, or start over with a new invite.', leaveP2p()) : undefined), JOIN_PATIENCE_MS);
    } else if (s === 'disconnected') {
      if (connectedOnce) closed(peer, 'guest');
    } else if (s === 'closed') closed(peer, 'guest');
  };
  try {
    await peer.pc.setRemoteDescription({ type: 'offer', sdp: code.sdp });
    await peer.pc.setLocalDescription(await peer.pc.createAnswer());
    await gathered(peer.pc);
    p2p.reply = await encodeCode({ kind: 'answer', sdp: peer.pc.localDescription!.sdp, sid: code.sid });
  } catch {
    leaveP2p();
    return 'That invite could not be used (is it all there, and was it made a moment ago?).';
  }
  live.status = 'connecting';
  live.room = 'peer session';
  p2p.waiting = 'Send the reply code below to the host. It connects by itself when they paste it.';
  return null;
}

/** Leaves the session (the cave stays open here as it is). */
export function leaveP2p() {
  stopSignal?.();
  stopSignal = null;
  clearTimeout(viewTimer);
  stopTimer();
  for (const p of peers.values()) {
    try {
      p.ch?.close();
      p.pc.close();
    } catch {
      /* already closed */
    }
  }
  peers.clear();
  pending.clear();
  wanted.clear();
  wantedAt.clear();
  relayTo.clear();
  guestPeer = null;
  const hadClass = cls !== null || live.people.some((p) => p.role);
  cls = null;
  if (hadClass) roomEvents.left();
  if (p2p.role !== 'none') {
    live.status = 'off';
    live.people = [];
    live.cursors = {};
    live.owner = false;
    live.ownerId = '';
  }
  p2p.role = 'none';
  p2p.invites = [];
  p2p.reply = '';
  p2p.waiting = '';
}

// Dev-only handle for tests and debugging in the browser console.
if (import.meta.env.DEV) (window as unknown as { __p2p: unknown }).__p2p = { p2p, makeInvite, takeReply, joinWithCode, letIn, startHosting, leaveP2p };
