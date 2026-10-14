// Editing a note's words in place: a small box over the words in the view, opened by a double click (or right after a leader is
// placed), saved with Enter or by clicking away, dropped with Escape.

export const textEdit = $state({ open: false, id: '', x: 0, y: 0, size: 14, angle: 0, value: '', placeholder: 'Type the note' });
/** When set, the box is for words that do not exist yet: Enter makes them with this (nothing is made from empty words or Escape). */
let makeNew: ((words: string) => void) | null = null;

/** The call that makes new words when the box is for new words, and clears it. */
export function takeMakeNew(): ((words: string) => void) | null {
  const f = makeNew;
  makeNew = null;
  return f;
}

/** Whether the box is for words that do not exist yet. */
export function isNew(): boolean {
  return makeNew !== null;
}

/** Opens the box for new words at a screen point; `make` is called with them if any are typed. */
export function openNewText(x: number, y: number, size: number, make: (words: string) => void, placeholder = 'Type the text', angle = 0) {
  openTextEdit('', '', x, y, size, placeholder, angle);
  makeNew = make;
}

/** Opens the box at a screen point (`x`, `y` are page pixels: where the words start, `size` their height in pixels). */
export function openTextEdit(id: string, value: string, x: number, y: number, size: number, placeholder = 'Type the note', angle = 0) {
  makeNew = null;
  textEdit.id = id;
  textEdit.value = value;
  textEdit.x = x;
  textEdit.y = y;
  textEdit.size = Math.max(11, Math.min(96, size));
  textEdit.angle = angle;
  textEdit.placeholder = placeholder;
  textEdit.open = true;
}

export function closeTextEdit() {
  makeNew = null;
  textEdit.open = false;
}
