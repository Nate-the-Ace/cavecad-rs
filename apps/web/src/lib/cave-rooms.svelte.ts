// The open cave's rooms and guest lists, for the Rooms panel. Owners add, rename and remove rooms and change who is
// on each guest list (and their seat); guests see the rooms they are in. The server enforces every rule
// (apps/server/src/rooms.ts); this only reads, asks, and says what went wrong in plain words.
import { callServer, HttpError, type Role } from './room-client';
import { serverAccess } from './room.svelte';

export type Seat = 'editor' | 'watcher';
export type Guest = { memberId: string; name: string; email: string; seat: Seat };
/** Someone invited by email who has not joined yet, with the seat kept for them (owners see these). */
export type Invited = { email: string; seat: Seat };
export type CaveRoom = { id: string; name: string; kind: 'private' | 'shared'; guests: Guest[]; invited: Invited[] };
export type Person = { memberId: string; name: string; email: string };
/** A CaveCAD member, as the guest picker lists them. */
export type Member = { id: string; name: string; email: string };

export const caveRooms = $state({
  /** The server id of the cave these rooms belong to ('' when none is read). */
  caveId: '',
  you: '' as Role | '',
  owners: [] as Person[],
  rooms: [] as CaveRoom[],
  /** Every CaveCAD member (owners only read it), for the guest picker. */
  members: [] as Member[],
  loaded: false,
  /** Why the rooms could not be read or a change was refused ('' when fine). */
  problem: '',
  busy: false,
  /** After inviting someone new: what happened, and the sign-up link when the email could not be sent. */
  sent: null as null | { email: string; emailed: boolean; link: string },
});

/** Server refusals in plain words. */
const WORDS: Record<string, string> = {
  'not-found': "That room or cave isn't there any more, or it isn't yours to change.",
  'bad-name': 'Give the room a name (up to 60 characters).',
  'name-taken': 'This cave already has a room with that name.',
  private: 'The Private room is for owners only: it cannot be renamed, removed or given guests.',
  'is-owner': 'That person owns this cave, so they are already in every room.',
  'no-member': "That person isn't a CaveCAD member. An admin invites new people first.",
  'admin-only': "That email isn't a CaveCAD member yet. Ask an admin to invite them.",
  'bad-email': "That doesn't look like an email address.",
  locked: 'Open this cave once on your own computer first: it was locked before caves moved to the server, and opening it unlocks it.',
};

function words(e: unknown): string {
  if (e instanceof HttpError) return WORDS[e.code] ?? `The server refused it (${e.code}).`;
  return "Can't reach the CaveCAD server.";
}

let reading = 0;

/** Reads the cave's rooms (and, for an owner, the members list). `caveId` '' clears. */
export async function readRooms(caveId: string): Promise<void> {
  const n = ++reading;
  if (!caveId) {
    Object.assign(caveRooms, { caveId: '', you: '', owners: [], rooms: [], loaded: false, problem: '' });
    return;
  }
  if (caveRooms.caveId !== caveId) Object.assign(caveRooms, { caveId, you: '', owners: [], rooms: [], loaded: false, problem: '' });
  const access = await serverAccess();
  if (n !== reading) return;
  if (!access) {
    Object.assign(caveRooms, { loaded: false, problem: 'Sign in to see who this cave is shared with.' });
    return;
  }
  try {
    const r = await callServer<{ you: Role; owners: Person[]; rooms: CaveRoom[] }>(access, 'GET', `/api/caves/${caveId}/rooms`);
    if (n !== reading) return;
    Object.assign(caveRooms, { you: r.you, owners: r.owners, rooms: r.rooms, loaded: true, problem: '' });
    if (r.you === 'owner' && caveRooms.members.length === 0) {
      const d = await callServer<{ members: Member[] }>(access, 'GET', '/api/members/directory');
      if (n === reading) caveRooms.members = d.members;
    }
  } catch (e) {
    if (n !== reading) return;
    caveRooms.loaded = false;
    caveRooms.problem = e instanceof HttpError && e.code === 'not-found' ? 'This cave is not on the server yet (it syncs once you are signed in and it has opened).' : words(e);
  }
}

/** Runs one owner change, then reads the rooms again. Gives back the new room's id for `addRoom`, '' otherwise, null when refused. */
async function change<T>(method: string, path: string, body?: unknown): Promise<T | null> {
  const caveId = caveRooms.caveId;
  const access = await serverAccess();
  if (!caveId || !access) {
    caveRooms.problem = 'Sign in to change rooms.';
    return null;
  }
  caveRooms.busy = true;
  let refused = '';
  try {
    return await callServer<T>(access, method, `/api/caves/${caveId}/rooms${path}`, body);
  } catch (e) {
    refused = words(e);
    return null;
  } finally {
    caveRooms.busy = false;
    await readRooms(caveId); // clears an old problem once the lists read fine
    if (refused) caveRooms.problem = refused; // kept past the read, so the person sees why
  }
}

/** Adds a room; gives back its id, or '' when refused. */
export const addRoom = async (name: string): Promise<string> => (await change<{ id: string }>('POST', '', { name }))?.id ?? '';
export const renameRoom = async (room: string, name: string): Promise<boolean> => !!(await change('PATCH', `/${room}`, { name }));
export const removeRoom = async (room: string): Promise<boolean> => !!(await change('DELETE', `/${room}`));
/** Puts a member on a room's guest list with a seat, or changes their seat. */
export const setGuest = async (room: string, member: string, seat: Seat): Promise<boolean> => !!(await change('PUT', `/${room}/guests/${member}`, { seat }));
export const removeGuest = async (room: string, member: string): Promise<boolean> => !!(await change('DELETE', `/${room}/guests/${member}`));
/**
 * Adds by email: a member is seated at once; anyone else is invited (admins only, the server says so otherwise) and
 * seated when they join. Says what happened in `caveRooms.sent` for an invite.
 */
export async function addByEmail(room: string, email: string, seat: Seat): Promise<boolean> {
  caveRooms.sent = null;
  const r = await change<{ added?: string; emailed?: boolean; link?: string; invite?: { email: string } }>('POST', `/${room}/invited`, { email, seat });
  if (r?.invite) caveRooms.sent = { email: r.invite.email, emailed: !!r.emailed, link: r.link ?? '' };
  return !!r;
}
export const cancelInvited = async (room: string, email: string): Promise<boolean> => !!(await change('DELETE', `/${room}/invited?email=${encodeURIComponent(email)}`));

/** Members matching what was typed (name or email), leaving out owners and those already on the room's list. */
export function matches(typed: string, room: CaveRoom | null): Member[] {
  const q = typed.trim().toLowerCase();
  if (!q || !room) return [];
  const taken = new Set([...caveRooms.owners.map((o) => o.memberId), ...room.guests.map((g) => g.memberId)]);
  return caveRooms.members.filter((m) => !taken.has(m.id) && (m.name.toLowerCase().includes(q) || m.email.toLowerCase().includes(q))).slice(0, 8);
}
