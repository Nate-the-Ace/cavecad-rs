// Who this person is to others: a name they can change and a colour. The account sets both from the profile
// after sign-in (`account.svelte.ts`); before that the name is a guess at the device (Laptop, iPad, Phone).

const NAME_KEY = 'cavecad.name';
export const COLORS = ['#3b82f6', '#e5484d', '#2fb86a', '#f0a020', '#8e6cef', '#14b8c4', '#e0559a', '#7c8a3a'];
const deviceName = () => (/iPad|Android(?!.*Mobile)/.test(navigator.userAgent) || (navigator.maxTouchPoints > 1 && /Mac/.test(navigator.platform)) ? 'iPad' : /Mobi/.test(navigator.userAgent) ? 'Phone' : 'Laptop');
const savedName = () => {
  try {
    return localStorage.getItem(NAME_KEY) || deviceName();
  } catch {
    return deviceName();
  }
};
const myId = Math.random().toString(36).slice(2, 10);

/** This device's id among the people in a cave. */
export const myPersonId = myId;

/** Who this device is to others: a name it can change, and a colour that follows its id. */
export const me = $state({ name: savedName(), color: COLORS[[...myId].reduce((n, c) => n + c.charCodeAt(0), 0) % COLORS.length] });

/** True when this browser has been given a name before. */
export function hasSavedName(): boolean {
  try {
    return !!localStorage.getItem(NAME_KEY);
  } catch {
    return false;
  }
}

/** Renames this device (remembered in this browser). */
export function rename(name: string) {
  me.name = name.slice(0, 40);
  try {
    localStorage.setItem(NAME_KEY, me.name);
  } catch {
    /* not remembered */
  }
}
