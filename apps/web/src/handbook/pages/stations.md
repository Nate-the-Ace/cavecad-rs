# Stations

The Stations panel lists every station in the cave, shows what kind of place each one is, and lets your team mark which leads are open, taken or finished.

## When you reach for it

- You want a list of leads and open ends to push on the next trip.
- You need to say who has a lead and what the team thinks of it.
- You want to find a station by name or by a note written at it.
- You want a checklist to take underground.

![The Stations panel](stations.png)

## How to use it

1. Open the **Stations** panel. It sits in the bottom strip beside the [Console](console) and the [Notebook](notebook).
2. Read the row for each station. It shows the station name, its kinds, the trips that reached it, its elevation and the note written in the survey.
3. Click a kind button above the table to show only that kind, such as **Lead** or **Open end**. The number on each button is how many stations have that kind. Click again to turn it off.
4. Type in **Search stations and notes** to find a name or a word in a note. The count beside it reads like "3 of 40".
5. Use the status list at the top to pick **Any status** or one status such as **open** or **pushed**.
6. In a row, choose a status in its own list: **open**, **assigned**, **pushed**, **closed** or **not a lead**.
7. Type in the **Team notes** box to say what the team thinks, and in the **Assigned** box to say who has the lead. Your change is saved when you click away.
8. Double-click a row to go to that station in the [Cross Section](section).

### Mark a lead as pushed

A lead that a later trip surveyed on from shows a small **pushed?** button next to its status. Click it to mark the lead as pushed.

### Take a checklist with you

1. Set the kind buttons, search and status so that only the stations you want are shown.
2. Click the download button (its tooltip starts "Download the stations shown as a checklist").
3. Open the CSV file in a spreadsheet or print it. It holds elevations only, never positions.

### Other things to try

- Right-click a row for a short menu, with **Show on the map**, **Cross section here** and **Start a new trip from** that station.
- Use the undo and redo buttons at the left of the bar to take back your last status or note.

## What can go wrong

| Message or symptom | What it means | Do this |
| --- | --- | --- |
| "Open a cave to see its stations." | No cave is open, so there is nothing to list. | Open or import a cave. See [Bringing survey data in](bringing-survey-data-in). |
| "No station matches." | Your kind buttons, search or status hide every row. | Turn off kind buttons, clear the search, or choose **Any status**. |
| A red message appears at the top of the bar | Your note or mark could not be saved. | Read the message, fix the text and try again. |
| The download button is greyed out | No station is shown, so there is nothing to put in the checklist. | Loosen the filters until some rows appear. |
| A lead shows "open (a lead)" with no status chosen | No one has marked it, so it counts as open. | Pick a status to mark it yourself. |

## Related

- [Notebook](notebook)
- [Cross Section](section)
- [Map](map)
- [Survey](survey)
