export interface Member {
  id: string;
  name: string;
  role: string;
}
export interface RoomIo {
  members(): Member[];
  send(id: string, msg: object): void;
  all(msg: object): void;
  others(msg: object): void;
}
export interface ClassState {
  name: string;
  /** Students may not keep the cave. */
  private: boolean;
  started: number;
  ended: boolean;
  seq: number;
  chat: { n: number; at: number; kind: string; id?: string; name?: string; role?: string; text: string }[];
  hands: Map<string, { name: string; since: number }>;
  mirror: string;
  lastSay: Map<string, number>;
  onRecord: ((entry: object) => void) | null;
}
export function newClass(name: string, onRecord?: (entry: object) => void, options?: { private?: boolean }): ClassState;
export function record(cls: ClassState, entry: object): ClassState['chat'][number];
export function handsOf(cls: ClassState): { id: string; name: string; since: number }[];
export function classWelcome(cls: ClassState, role: string): { role: string; name: string; private: boolean; started: number; ended: boolean; chat: ClassState['chat']; hands: ReturnType<typeof handsOf>; mirror: string };
export function classMessage(cls: ClassState, sender: Member, msg: { t: string } & Record<string, any>, io: RoomIo): boolean;
export function classJoined(cls: ClassState, person: { id: string; name: string; role?: string }, io: RoomIo): void;
export function classLeft(cls: ClassState, id: string, io: RoomIo, person?: { name: string; role?: string }): void;
export const CLASS_MESSAGES: string[];
