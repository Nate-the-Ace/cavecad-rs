// Editing a note's words in place: a small box over the words in the view, opened by a double click (or right after a leader is
// placed), saved with Enter or by clicking away, dropped with Escape.

export const textEdit = $state({ open: false, id: '', x: 0, y: 0, size: 14, value: '', placeholder: 'Type the note' });

/** Opens the box at a screen point (`x`, `y` are page pixels: where the words start, `size` their height in pixels). */
export function openTextEdit(id: string, value: string, x: number, y: number, size: number, placeholder = 'Type the note') {
  textEdit.id = id;
  textEdit.value = value;
  textEdit.x = x;
  textEdit.y = y;
  textEdit.size = Math.max(11, Math.min(40, size));
  textEdit.placeholder = placeholder;
  textEdit.open = true;
}

export function closeTextEdit() {
  textEdit.open = false;
}
