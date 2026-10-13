# Class mailbox (signaling)

A small Google Apps Script that lets CaveCAD pages which have never met pass each other the first few
notes of a direct connection, so one class link can bring every student in. Owned by
**cavecad.app@gmail.com**, as its own project (the feedback receiver has its own `doPost`). The
decisions are in `Logic.gs` (tested: `node --test 'tools/signal-mailbox/test/*.test.js'`); `Code.gs` wires
them to Google's cache and lock. `Code.combined.gs` is both files in one, for pasting.

## What it stores, and what it never sees

Three kinds of note, named `hello:ID`, `offer:ID` and `answer:ID` (ID is one student's attempt), each at most
12,000 characters, kept in the script cache for 30 minutes and then gone, under a session id that is the
unguessable part of the class link. An offer or answer holds a device's network addresses and encryption
fingerprint, as it would in any video call. The cave, the chat, the teacher's view and the people are never
sent here: once two pages have swapped their notes they talk to each other directly (or through the TURN relay,
if a strict network blocks a direct path).

## One-time setup (signed in as cavecad.app@gmail.com)

1. script.google.com > New project `CaveCAD Class Mailbox`. Add files `Logic.gs` and `Code.gs` with this
   folder's contents (or one file with `Code.combined.gs`).
2. Project Settings > Script properties: `KEY` = a long random string, e.g.
   `python3 -c "import secrets;print(secrets.token_urlsafe(32))"`.
3. Deploy > New deployment > Web app. Execute as: Me. Who has access: Anyone. Copy the `/exec` URL.
   Approve the permissions it asks for (it needs none beyond the script's own cache).
4. GitHub `Nate-the-Ace/cavecad-rs` > Settings > Secrets > Actions: `SIGNAL_ENDPOINT` = the `/exec` URL,
   `SIGNAL_KEY` = the KEY. The Pages and desktop builds pass them to the app as `VITE_SIGNAL_ENDPOINT` and
   `VITE_SIGNAL_KEY`.
5. To try a deployment without a rebuild: Start a Classroom > "A link, then direct" lets you type the address
   and key in; they are kept on that machine and travel inside the class link so students' copies use them too.

Redeploying after a code change: Deploy > Manage deployments > edit > Version: New. The `/exec` URL stays.

## Calls

Plain requests that need no permission step from the browser (GET to read, POST of plain text to write):
`?op=put&k=KEY&s=SESSION&n=NAME` with the note as the body; `op=get` with `n`; `op=list` with `p` (a name
prefix; returns the notes with their values); `op=del` with `n`. Every reply is JSON, `{ok:true,...}` or
`{ok:false,error:"key"|"session"|"name"|"too-big"|"busy"|"empty"|"op"}`.

## Limits

About 900 calls a minute for the whole mailbox, 400 notes a session. Each call runs a fraction of a second, so a
class of thirty uses a few seconds of the day's script time. A very large or very frequent use could reach
Google's daily script quota; the mailbox then answers with errors and the app says so in words.
