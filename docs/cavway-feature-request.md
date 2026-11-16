Subject: Cavway X1 firmware: four small Bluetooth additions for app integration

Hello Siwei,

I'm Nathan Schonegg. I build CaveCAD (cavecad.com), a cave survey and mapping app for desktop, iPad and iPhone. It
now connects directly to the Cavway X1 over Bluetooth and writes shots straight into the survey notebook. Thank you
for publishing the manual, the communication protocol and Cavway Assistant: they made this possible, and the X1 has
been excellent to work with.

While building the integration with my own X1 (CavwayX1-0312, firmware 1.1.6), I found four things that only the
firmware can do. Each would help any app that works with the X1, TopoDroid included:

1. **Start and stop scan mode over Bluetooth.** We already use your documented single-byte commands (0x36 laser on,
   0x37 laser off, 0x38 take a shot). Two more, for example "enter scan mode" and "exit scan mode", would let an app
   start a sweep with one tap while the surveyor holds the X1 steady, rather than asking them to hold DIST+M.

2. **A message when a scan ends.** We recognise scan shots by the flag byte (bit 6 clear, and bit 5 also clear on the
   first shot of a scan). Nothing says when the scan has finished (CLR, the time limit, or 200 shots), so today we
   guess from a pause in the shots. A small packet or flag on the last shot would make this exact.

3. **Stay awake while connected.** When the idle time-off is reached the X1 switches off, even with an app connected
   and a survey under way. An option to stay on while a Bluetooth connection is open, or a command an app can send
   to reset the idle timer, would keep a survey session from dropping.

4. **Calibration shots sent live.** We would like to guide the surveyor through calibration on the phone (which
   direction to point next, as your two circles do on the X1 screen). Could you confirm that calibration shots are
   sent over Bluetooth as they are taken, in Calibration mode, with the calibration flag (bit 0) clear? If not,
   sending them that way would be enough. A short message when a group is reset with the > button would also help.

If any of these are already possible and I have missed them, I'd be glad to know. I'm also happy to test a beta
firmware with my X1 and send you Bluetooth captures.

Thank you again for a great instrument.

Best regards,
Nathan Schonegg
CaveCAD (cavecad.com)
