# Console

The Console panel shows a running list of what CaveCAD is doing, so you can see why something did not work.

## When you reach for it

- Something failed and you want to know why.
- You are sending a problem report and want to copy the details.
- You want to check that an import or a redraw really ran.

![The Console panel](console.png)

## How to use it

1. Open the Console panel. New lines appear at the bottom as things happen.
2. Pick how much you want to see from the level menu. It starts at **Info**.
3. Choose **Warnings** to see only things that may be wrong, or **Errors** to see only failures.
4. Choose **Detail** or **Debug** for more, which is mostly useful when you are chasing a problem.
5. Type in **Search** to keep only lines that contain your words. It looks in the source and the message.
6. Press **Copy** to put the lines you can see on the clipboard, ready to paste into a message.
7. Press **Clear** to empty the list.

The list follows the newest line. Scroll up to stop it. Scroll back to the bottom to follow again.

A line said many times in a row is shown once, with a count such as ×3. Warnings and errors are coloured.

## What can go wrong

| Message or symptom | What it means | Do this |
| --- | --- | --- |
| The list is empty | Nothing has happened at this level, or the cave tools are still loading. | Choose **Debug** to show everything, or wait a moment. |
| A line you expect is missing | The level menu or **Search** is hiding it. | Clear the search box and lower the level. |
| The list stopped moving | You scrolled up, so it stopped following. | Scroll to the bottom. |
| **Clear** removed the evidence | **Clear** empties the list for good. | Press **Copy** first if you need the lines. |

## Related

- [Bringing survey data in](bringing-survey-data-in)
- [Notebook](notebook)
