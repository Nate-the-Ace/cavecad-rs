// The phone Notebook display setting's parse/serialize (lib/notebook-display.ts). Run: npm run test:notebookdisplay
import { BACKSIGHTS_KEY, notebookBacksightsChange, parseBacksights, parseSurveyBacksights, SURVEY_BACKSIGHTS_KEY, serializeBacksights, surveyBacksightsChange } from '../src/lib/notebook-display.ts';

let failures = 0;
function check(what: string, ok: boolean) {
  if (!ok) failures += 1;
  console.log(`${ok ? 'ok  ' : 'FAIL'} ${what}`);
}
check('key', BACKSIGHTS_KEY === 'cavecad.notebook.backsights');
check('survey key', SURVEY_BACKSIGHTS_KEY === 'cavecad.survey.backsights');
check('survey choice: only 1 is on, default off', parseSurveyBacksights('1') && !parseSurveyBacksights('0') && !parseSurveyBacksights(null) && !parseSurveyBacksights(undefined) && !parseSurveyBacksights('x'));
check('nothing stored: visibility follows the survey default', !parseBacksights(null, false) && !parseBacksights(undefined, false) && !parseBacksights('', false) && parseBacksights(null, true));
check('stored 0/1 wins over the fallback', !parseBacksights('0', true) && parseBacksights('1', false));
check('junk follows the fallback', parseBacksights('x', true) && !parseBacksights('x', false));
check('round trip', parseBacksights(serializeBacksights(true), false) && !parseBacksights(serializeBacksights(false), true));

// walk the two settings through a session with a tiny model of the stores (storage = a map)
{
  const store = new Map<string, string>();
  const launch = () => ({
    collect: parseSurveyBacksights(store.get(SURVEY_BACKSIGHTS_KEY)),
    show: parseBacksights(store.get(BACKSIGHTS_KEY), parseSurveyBacksights(store.get(SURVEY_BACKSIGHTS_KEY))),
  });
  let st = launch();
  check('fresh start: both off', !st.collect && !st.show);
  const surveyToggle = (on: boolean) => {
    const c = surveyBacksightsChange(on);
    st.collect = c.collect;
    st.show = c.show;
    store.set(SURVEY_BACKSIGHTS_KEY, serializeBacksights(c.collect));
    store.set(BACKSIGHTS_KEY, serializeBacksights(c.show));
  };
  const notebookToggle = (on: boolean) => {
    const c = notebookBacksightsChange(on);
    if (c.collect !== null) st.collect = c.collect;
    st.show = c.show;
    store.set(BACKSIGHTS_KEY, serializeBacksights(c.show));
  };
  surveyToggle(true);
  check('survey toggle on once: both on', st.collect && st.show);
  notebookToggle(false);
  check('notebook toggle off: visibility only', st.collect && !st.show);
  notebookToggle(true);
  check('notebook toggle on: visibility only', st.collect && st.show);
  notebookToggle(false);
  st = launch();
  check('relaunch: both restored (collect on, hidden)', st.collect && !st.show);
  store.clear();
  surveyToggle(true);
  surveyToggle(false);
  st = launch();
  check('survey off then relaunch: both off', !st.collect && !st.show);
}
check('survey toggle sets collection and visibility to its value', JSON.stringify(surveyBacksightsChange(false)) === '{"collect":false,"show":false}' && JSON.stringify(surveyBacksightsChange(true)) === '{"collect":true,"show":true}');
check('notebook toggle changes visibility only', notebookBacksightsChange(false).collect === null && !notebookBacksightsChange(false).show && notebookBacksightsChange(true).collect === null && notebookBacksightsChange(true).show);
if (failures) {
  console.log(`${failures} failed`);
  process.exit(1);
}
