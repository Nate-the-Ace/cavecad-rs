// The phone Notebook display setting's parse/serialize (lib/notebook-display.ts). Run: npm run test:notebookdisplay
import { BACKSIGHTS_KEY, notebookBacksightsChange, parseBacksights, serializeBacksights, surveyBacksightsChange } from '../src/lib/notebook-display.ts';

let failures = 0;
function check(what: string, ok: boolean) {
  if (!ok) failures += 1;
  console.log(`${ok ? 'ok  ' : 'FAIL'} ${what}`);
}
check('key', BACKSIGHTS_KEY === 'cavecad.notebook.backsights');
check('default is on', parseBacksights(null) && parseBacksights(undefined) && parseBacksights(''));
check('0 is off, 1 is on', !parseBacksights('0') && parseBacksights('1'));
check('junk stays on', parseBacksights('false') && parseBacksights('x'));
check('round trip', parseBacksights(serializeBacksights(true)) && !parseBacksights(serializeBacksights(false)));
check('survey toggle sets collection and visibility to its value', JSON.stringify(surveyBacksightsChange(false)) === '{"collect":false,"show":false}' && JSON.stringify(surveyBacksightsChange(true)) === '{"collect":true,"show":true}');
check('notebook toggle changes visibility only', notebookBacksightsChange(false).collect === null && !notebookBacksightsChange(false).show && notebookBacksightsChange(true).collect === null && notebookBacksightsChange(true).show);
if (failures) {
  console.log(`${failures} failed`);
  process.exit(1);
}
