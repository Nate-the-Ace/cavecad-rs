# Invite mailer

Sends CaveCAD invite emails from cavecad.app@gmail.com. Its own Apps Script project, set up like
`tools/feedback-receiver`.

1. Signed in as cavecad.app@gmail.com, create a new Apps Script project named "CaveCAD invite mailer".
2. Paste `Code.combined.gs` into the project's Code.gs (or add `Logic.gs` and `Code.gs` as two files).
3. Run `setUp` once from the editor and approve the permission (send email as you). It makes the `KEY`
   script property and sets `SITE` = `https://cave.schonegg.stream`.
4. Deploy > New deployment > Web app: execute as Me, who has access: Anyone.
5. Give the CaveCAD server the web app URL as `MAILER_URL` and the `KEY` (Project Settings > Script
   properties) as `MAILER_KEY`, typed in by the person who can see it, not shown anywhere else.

Consumer Gmail sends about 100 emails a day; the script also caps itself at 40 an hour. Test the
decisions with `node tools/invite-mailer/test.mjs`.
