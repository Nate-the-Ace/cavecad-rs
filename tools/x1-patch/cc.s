@ CaveCAD scan control for Cavway X1 firmware 1.1.7 (public build, sha256 18060d77…): "1.1.7-cc4".
@ Assembled by build.py, placed at PATCH_BASE in free flash after the stock image. Addresses of the stock code are
@ in the .set lines below; every one is explained in tools/cave-x1/FIRMWARE.md ("Scan mode in 1.1.7").
@
@ New Bluetooth/USB commands (single byte, framed like every other command):
@   39  start scan   (needs laser on and ready: send 36 first and wait for it)
@   3A  stop scan    (as if CLR were pressed)
@   3B  status
@ Each answers like a memory read of address 0xCC00 + cmd (the USB link only forwards replies that start with 3D):
@   [3D, cmd, CC, 12, mode, laser, count lo, count hi, ticks x4, options, result, last end, cc version]
@ (cc1 answered [cmd, 0, 0, 12, ...]: Bluetooth only, the USB sender dropped it.)
@   result: 0 done, 1 wrong mode, 2 laser not ready
@   last end: 0 none since power on, 1 CLR, 2 time limit, 3 shot limit, 4 remote stop (3A)
@ Scan shots carry the scan counter in packet bytes 54-55 (u16, first shot = 1); other shots keep 0xFFFF there.

        .syntax unified
        .thumb
        .text

        @ stock 1.1.7 code: addresses given by build.py (STOCK), since branches to them need the final placement
        @   DISP_MEASURE 0x0800c3e2  dispatcher: the 38/35 "measure" path
        @   DISP_EXIT    0x0800c49a  dispatcher: common exit (idle reload, return [sp+0x88])
        @   BEEP         0x0800ca40  beep pattern r0
        @   LASER_CONT   0x08016098  start continuous measuring (acts only when laser state == 3)
        @   STORE_RECORD 0x08013448  writes a built shot record (r0 = record)
        @   SET_DEFAULTS_2 0x0800c520  second half of the factory-defaults routine (no arguments)
        @   SAVE_SETTINGS 0x0800c88c   writes the 24-byte settings block to the EEPROM (no arguments)
        .extern DISP_MEASURE, DISP_EXIT, BEEP, LASER_CONT, STORE_RECORD, SET_DEFAULTS_2, SAVE_SETTINGS

        .set USB_OK,        0x20000024      @ u32: stock sets it from the charger pin (1 = charging)
        .set USB_REPLY,     0x20001624      @ USB reply buffer: [0] == 0x3D while a reply waits

        @ stock 1.1.7 RAM
        .set MODE,          0x20000068      @ u8: 1 laser on, 2 scan
        .set SCAN_COUNT,    0x20000070      @ u32: shots stored this scan
        .set SHOT_DELAY,    0x200000a0      @ u16
        .set SCAN_TICKS,    0x200000c0      @ u32: 25 ms ticks since scan began
        .set MARK,          0x20000098      @ u8: shot mark, 0xff none
        .set LASER_STATE,   0x200000ec      @ u8: 3 = on and ready
        .set CLR_EVENT,     0x200012b9      @ u8: CLR key event (1 = press)
        .set SETTINGS,      0x20001044      @ options byte at +0x16
        .set TX_FLAGS,      0x20000110      @ u16: bit 2 = a reply is waiting

        @ cc state, in RAM no stock code uses (above the stack top 0x2000a880)
        .set CC_STATE,      0x2000bf00      @ +0 magic 0xA5, +1 last end reason, +2 remote stop pending,
                                            @ +3 Scan Options before a remote scan, +4 1 = restore them at the end
        .set CC_MAGIC,      0xa5
        .set CC_VERSION,    4
        .set CC_SETTINGS_MARK, 0xc4         @ settings byte 0x17 (unused by stock): CaveCAD defaults applied once
        .set OPT_SCAN,      0x16            @ settings: Scan Options byte
        @ remote scans: steadiness level 2 (10000, the most lenient), time limit index 3 (10 min), allowed
        .set OPT_REMOTE,    0x35

@ ---------------------------------------------------------------- dispatcher hook
@ Replaces "cmp r4,#0x35 / bne exit" at 0x0800c3de with "b.w cc_dispatch". r4 = command byte,
@ r7 = reply buffer, [sp+0x88] = "reply waiting" (the dispatcher's own frame: we arrived by b.w).
        .global cc_dispatch
        .thumb_func
cc_dispatch:
        cmp     r4, #0x35
        bne     1f
        b.w     DISP_MEASURE
1:      cmp     r4, #0x39
        beq     cc_start
        cmp     r4, #0x3a
        beq     cc_stop
        cmp     r4, #0x3b
        beq     cc_status
        b.w     DISP_EXIT

cc_start:
        ldr     r0, =MODE
        ldrb    r1, [r0]
        cmp     r1, #1
        bne     cc_wrong_mode
        ldr     r2, =LASER_STATE
        ldrb    r2, [r2]
        cmp     r2, #3
        bne     cc_laser_not_ready
        @ what the DIST+M entry does (0x0800d230..0x0800d250), without the key test
        movs    r1, #2
        strb    r1, [r0]
        movs    r1, #0
        ldr     r2, =SCAN_COUNT
        str     r1, [r2]
        ldr     r2, =SHOT_DELAY
        strh    r1, [r2]
        ldr     r2, =SCAN_TICKS
        str     r1, [r2]
        movs    r1, #0xff
        ldr     r2, =MARK
        strb    r1, [r2]
        bl      cc_state_init
        ldr     r2, =CC_STATE
        movs    r1, #0
        strb    r1, [r2, #2]
        @ remote scans glide: the most lenient steadiness and the longest time limit, the user's own put back after
        ldrb    r1, [r2, #4]
        cbnz    r1, 12f                 @ already swapped (a restart inside one sweep): keep the saved original
        ldr     r3, =SETTINGS
        ldrb    r1, [r3, #OPT_SCAN]
        strb    r1, [r2, #3]
        movs    r1, #1
        strb    r1, [r2, #4]
        movs    r1, #OPT_REMOTE
        strb    r1, [r3, #OPT_SCAN]
12:     movs    r0, #9
        bl      BEEP
        bl      LASER_CONT
        movs    r5, #0
        b       cc_reply

cc_stop:
        ldr     r0, =MODE
        ldrb    r1, [r0]
        cmp     r1, #2
        bne     cc_wrong_mode
        bl      cc_state_init
        ldr     r2, =CC_STATE
        movs    r1, #1
        strb    r1, [r2, #2]
        ldr     r2, =CLR_EVENT
        strb    r1, [r2]
        movs    r5, #0
        b       cc_reply

cc_status:
        movs    r5, #0
        b       cc_reply
cc_wrong_mode:
        movs    r5, #1
        b       cc_reply
cc_laser_not_ready:
        movs    r5, #2

@ reply: [3D, cmd, CC, 12, mode, laser, count u16, ticks u32, options, result, last end, version]
cc_reply:
        movs    r0, #0x3d
        strb    r0, [r7]
        strb    r4, [r7, #1]
        movs    r0, #0xcc
        strb    r0, [r7, #2]
        movs    r0, #12
        strb    r0, [r7, #3]
        ldr     r0, =MODE
        ldrb    r0, [r0]
        strb    r0, [r7, #4]
        ldr     r0, =LASER_STATE
        ldrb    r0, [r0]
        strb    r0, [r7, #5]
        ldr     r0, =SCAN_COUNT
        ldr     r0, [r0]
        strb    r0, [r7, #6]
        lsrs    r0, r0, #8
        strb    r0, [r7, #7]
        ldr     r0, =SCAN_TICKS
        ldr     r0, [r0]
        strb    r0, [r7, #8]
        lsrs    r0, r0, #8
        strb    r0, [r7, #9]
        lsrs    r0, r0, #8
        strb    r0, [r7, #10]
        lsrs    r0, r0, #8
        strb    r0, [r7, #11]
        ldr     r0, =SETTINGS
        ldrb    r0, [r0, #0x16]
        strb    r0, [r7, #12]
        strb    r5, [r7, #13]
        movs    r0, #0
        ldr     r1, =CC_STATE
        ldrb    r2, [r1]
        cmp     r2, #CC_MAGIC
        bne     2f
        ldrb    r0, [r1, #1]
2:      strb    r0, [r7, #14]
        movs    r0, #CC_VERSION
        strb    r0, [r7, #15]
        @ as the 3d path does: reply waiting
        movs    r0, #1
        str     r0, [sp, #0x88]
        ldr     r0, =TX_FLAGS
        ldrh    r1, [r0]
        orr     r1, r1, #4
        strh    r1, [r0]
        b.w     DISP_EXIT

@ cc state: after power-on the RAM holds anything; the first use marks it valid with "no scan ended yet".
        .thumb_func
cc_state_init:
        ldr     r2, =CC_STATE
        ldrb    r3, [r2]
        cmp     r3, #CC_MAGIC
        beq     10f
        movs    r3, #CC_MAGIC
        strb    r3, [r2]
        movs    r3, #0
        strb    r3, [r2, #1]
        strb    r3, [r2, #2]
        strb    r3, [r2, #4]
10:     bx      lr

@ ---------------------------------------------------------------- scan end hook
@ Replaces "movs r0,#0 / ldr r1,=SCAN_TICKS" at 0x0800d500 (the scan's exit block) with "bl cc_scan_end".
@ Records why the scan ended, then returns with r0 = 0, r1 = SCAN_TICKS as the replaced code left them.
        .global cc_scan_end
        .thumb_func
cc_scan_end:
        ldr     r2, =CC_STATE
        ldrb    r3, [r2]
        cmp     r3, #CC_MAGIC
        beq     3f
        movs    r3, #CC_MAGIC
        strb    r3, [r2]
        movs    r3, #0
        strb    r3, [r2, #1]
        strb    r3, [r2, #2]
        strb    r3, [r2, #4]
3:      ldr     r0, =CLR_EVENT
        ldrb    r0, [r0]
        cmp     r0, #1
        bne     4f
        movs    r3, #1                  @ CLR...
        ldrb    r0, [r2, #2]
        cmp     r0, #0
        beq     6f
        movs    r3, #4                  @ ...sent by 3A
        b       6f
4:      ldr     r0, =SCAN_COUNT
        ldr     r0, [r0]
        cmp     r0, #200
        ble     5f
        movs    r3, #3                  @ shot limit
        b       6f
5:      movs    r3, #2                  @ time limit
6:      strb    r3, [r2, #1]
        movs    r3, #0
        strb    r3, [r2, #2]
        ldrb    r3, [r2, #4]            @ a remote scan: put the user's Scan Options back
        cbz     r3, 13f
        ldrb    r3, [r2, #3]
        ldr     r0, =SETTINGS
        strb    r3, [r0, #OPT_SCAN]
        movs    r3, #0
        strb    r3, [r2, #4]
13:     movs    r0, #0
        ldr     r1, =SCAN_TICKS
        bx      lr

@ ---------------------------------------------------------------- shot counter in packets
@ Replaces "bl STORE_RECORD" at 0x080132c4 with "bl cc_store". r0 = the built 64-byte record.
        .global cc_store
        .thumb_func
cc_store:
        ldr     r1, =MODE
        ldrb    r1, [r1]
        cmp     r1, #2
        bne     7f
        ldr     r1, =SCAN_COUNT
        ldr     r1, [r1]
        strb    r1, [r0, #54]
        lsrs    r1, r1, #8
        strb    r1, [r0, #55]
7:      b.w     STORE_RECORD

@ ---------------------------------------------------------------- USB answers without charging
@ Replaces "ldr r0,=USB_OK / ldr r0,[r0]" at 0x0800641a (the USB sender's gate) with "bl cc_usb_gate".
@ Stock sends over USB only while charging (the charger pin stands in for "cable present"). A reply that is waiting
@ means a command just came in over USB, so a computer is there: send it. Shots still go only while charging.
        .global cc_usb_gate
        .thumb_func
cc_usb_gate:
        ldr     r0, =USB_OK
        ldr     r0, [r0]
        cbnz    r0, 11f
        ldr     r1, =USB_REPLY
        ldrb    r1, [r1]
        cmp     r1, #0x3d
        bne     11f
        movs    r0, #1
11:     bx      lr

@ ---------------------------------------------------------------- defaults: sleep 300 s, beep off
@ Replaces "bl SET_DEFAULTS_2" at 0x0800c682, inside the factory-defaults routine (Reset Default, and first boot
@ with blank settings). Stock sets IdleOff index 0 (60 s) and beep on; cc3 sets index 4 (300 s) and silent.
        .global cc_defaults
        .thumb_func
cc_defaults:
        ldr     r1, =SETTINGS
        movs    r0, #4                  @ IdleOff: 60/120/180/240/300 s, index 4
        strb    r0, [r1, #4]
        movs    r0, #1                  @ +6 = 1: silent (BEEP returns at once)
        strb    r0, [r1, #6]
        movs    r0, #CC_SETTINGS_MARK   @ these are CaveCAD's defaults: nothing to migrate later
        strb    r0, [r1, #0x17]
        b.w     SET_DEFAULTS_2

@ ---------------------------------------------------------------- boot: CaveCAD defaults, once
@ Replaces "ldr r0,=0x200010dc / ldrb r0,[r0]" at 0x08015bc0, just after the settings are read from the EEPROM.
@ Once per X1 (marker in settings byte 0x17): a setting still at Cavway's default takes CaveCAD's (beep on -> off,
@ sleep 60 s -> 300 s); one the user changed is kept. Saved at once, so later changes (beep back on) stay.
        .global cc_settings_boot
        .thumb_func
cc_settings_boot:
        push    {r4, lr}
        ldr     r1, =SETTINGS
        ldrb    r0, [r1]
        cmp     r0, #0xff               @ blank: the factory defaults run next (cc_defaults)
        beq     15f
        ldrb    r0, [r1, #0x17]
        cmp     r0, #CC_SETTINGS_MARK
        beq     15f
        ldrb    r0, [r1, #6]            @ beep: 0 (on) is Cavway's default
        cbnz    r0, 14f
        movs    r0, #1
        strb    r0, [r1, #6]
14:     ldrb    r0, [r1, #4]            @ sleep: index 0 (60 s) is Cavway's default
        cbnz    r0, 16f
        movs    r0, #4
        strb    r0, [r1, #4]
16:     movs    r0, #CC_SETTINGS_MARK
        strb    r0, [r1, #0x17]
        bl      SAVE_SETTINGS
15:     ldr     r0, =0x200010dc         @ what the replaced instructions loaded
        ldrb    r0, [r0]
        pop     {r4, pc}

        .ltorg
