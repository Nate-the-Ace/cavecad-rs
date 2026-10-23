# Invite mailer

Sends CaveCAD invite emails from cavecad.app@gmail.com. Its own Apps Script project, set up like
`tools/feedback-receiver`.

1. Signed in as cavecad.app@gmail.com, create a new Apps Script project named "CaveCAD invite mailer".
2. Add `Logic.gs` and `Code.gs` from this folder (same names).
3. Project settings > Script properties: `KEY` = a long random string, `SITE` = `https://cave.schonegg.stream`.
4. Deploy > New deployment > Web app: execute as Me, who has access: Anyone. Approve the Gmail permission.
5. Give the CaveCAD server the web app URL as `MAILER_URL` and the key as `MAILER_KEY`.

Consumer Gmail sends about 100 emails a day; the script also caps itself at 40 an hour. Test the
decisions with `node tools/invite-mailer/test.mjs`.
