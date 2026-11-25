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
        .extern UI_CONT, UI_EXIT, DRAW_PREP, SET_FONT, DRAW_STR
        .extern STOCK_VALIDATE, STOCK_WRITE, FLASH_WRITE_HALF, BUILD_REC

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
        @ cc5 feature picker: CC_STATE +5 = list cursor, +6 = callout chosen for the next shot (1-based, 0 none)
        .set MODE_PICK,     0x40            @ a UI mode of our own: no stock code or table uses it
        .set NAME_BYTES,    13              @ one list entry: up to 12 characters and a 0
        .set MAX_NAMES,     32
        @ cc8 list in flash: x1flash `callouts push` writes a list into the X1 without a reflash. A computer writes it with the
        @ stock memory-write (3e) at 0xC000..0xC1FF (a RAM buffer), then writes C0 to 0xC200 to commit it, and reads
        @ 0xC200 for the result [result, slot, count, cc version]: 0 ok, 2 not a valid list, 3 no room, 4 slot not erased, 5 write failed.
        @ Each commit goes into the next empty 512-byte slot of the (otherwise unused) flash sector 5, never erasing anything;
        @ the newest valid slot is the list the picker shows, else the one built in below.
        .set LIST_ADDR,     0xc000
        .set LIST_SPAN,     0x210
        .set LIST_READ,     0xc400          @ cc9: reads here show the list IN USE (flash slot or built in), 426 bytes
        .set CC_LISTBUF,    0x2000c000      @ RAM above everything stock uses and cc's state
        .set CC_STATUS,     0x2000c200
        .set FLASH_LIST,    0x08020000
        .set FLASH_LIST_END, 0x0803fe00     @ 255 slots; the app's marker word at 0x0803ff00 is never touched
        .set SLOT,          512
        .set BLOCK_LEN,     424             @ "CCAL", count, names
        .set SLOT_LEN,      426             @ and a CRC-16/Modbus of those bytes
        .set KEY_M,         0x2000129a      @ key structs: byte +1 is the event (1 press, 2 long press)
        .set KEY_LT,        0x200012a4
        .set KEY_GT,        0x200012ae
        .set KEY_CLR,       0x200012b8
        .set FONT_TITLE,    0x080193a7      @ the arguments stock menus give SET_FONT (0x0800cf88)
        .set FONT_ROW,      0x0801874a
        .set CC_VERSION,    10
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
        strb    r3, [r2, #5]
        strb    r3, [r2, #6]
        str     r3, [r2, #8]            @ cached pointer to the list in use (0: look it up)
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
@ Replaces "bl STORE_RECORD" at 0x080132c4 (scan shots) and at 0x0800791a (normal shots, from the record buffer at
@ 0x20000c44) with "bl cc_store". r0 = the built 64-byte record.
        .global cc_store
        .thumb_func
cc_store:
        ldr     r1, =MODE
        ldrb    r1, [r1]
        cmp     r1, #2
        bne     6f
        ldr     r1, =SCAN_COUNT
        ldr     r1, [r1]
        strb    r1, [r0, #54]
        lsrs    r1, r1, #8
        strb    r1, [r0, #55]
        b.w     STORE_RECORD
        @ cc5: a callout picked on the X1 rides on the next shot: byte 54 = its number (1 to 16), byte 55 = 0xCA
6:      ldr     r2, =CC_STATE
        ldrb    r3, [r2]
        cmp     r3, #CC_MAGIC
        bne     7f
        ldrb    r3, [r2, #6]
        cbz     r3, 7f
        strb    r3, [r0, #54]
        movs    r3, #0xca
        strb    r3, [r0, #55]
        movs    r3, #0
        strb    r3, [r2, #6]
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


@ ---------------------------------------------------------------- feature picker (cc5, list as data in cc7)
@ Hold `<` for 2 s with the laser on (mode 1) to open a list of cave features; `<` `>` scroll, M picks, CLR cancels.
@ The next shot carries the pick (cc_store). Two hooks: cc_ui (the UI state machine's entry) and cc_draw (the screen
@ drawing function's first call). Mode 0x40 is ours alone. The list is DATA (callout_block, below): x1flash
@ `callouts set` rewrites it in the .bin, so a list can be changed and reflashed without rebuilding this file.
@
@ cc_ui replaces "push {r4,lr} / sub sp,#0x28" at 0x0800cfe0 (b.w here). It does those two instructions itself, so
@ a frame exists for either exit: back into the stock handler at 0x0800cfe4, or the common exit at 0x0800e7c8.
        .global cc_ui
        .thumb_func
cc_ui:
        push    {r4, lr}
        sub     sp, #0x28
        ldr     r2, =MODE
        ldrb    r1, [r2]
        cmp     r1, #MODE_PICK
        beq     20f
        cmp     r1, #1
        bne     29f
        ldr     r3, =KEY_LT
        ldrb    r3, [r3, #1]
        cmp     r3, #2                  @ long press of `<`
        bne     29f
        bl      cc_list
        ldrb    r3, [r0, #4]            @ an empty list: nothing to open
        cmp     r3, #0
        beq     29f
        bl      cc_state_init
        ldr     r3, =CC_STATE
        movs    r0, #0
        strb    r0, [r3, #5]            @ cursor at the top
        movs    r0, #MODE_PICK
        ldr     r2, =MODE
        strb    r0, [r2]
        b.w     UI_EXIT
29:     b.w     UI_CONT
20:     ldr     r4, =CC_STATE
        ldrb    r0, [r4, #5]            @ cursor
        ldr     r3, =KEY_LT
        ldrb    r3, [r3, #1]
        cmp     r3, #1
        bne     21f
        cbz     r0, 21f
        subs    r0, #1
21:     ldr     r3, =KEY_GT
        ldrb    r3, [r3, #1]
        cmp     r3, #1
        bne     22f
        push    {r0}
        bl      cc_list
        ldrb    r1, [r0, #4]            @ count
        pop     {r0}
        subs    r1, #1                  @ last index
        cmp     r0, r1
        bhs     22f
        adds    r0, #1
22:     strb    r0, [r4, #5]
        ldr     r3, =KEY_M
        ldrb    r3, [r3, #1]
        cmp     r3, #1
        bne     23f
        adds    r0, #1
        strb    r0, [r4, #6]            @ the pick, 1-based
        movs    r0, #9
        bl      BEEP
        b       24f
23:     ldr     r3, =KEY_CLR
        ldrb    r3, [r3, #1]
        cmp     r3, #1
        bne     25f
24:     ldr     r2, =MODE
        movs    r3, #1
        strb    r3, [r2]                @ back to the measure screen
25:     b.w     UI_EXIT

@ cc_draw replaces "bl DRAW_PREP" at 0x080072ba (the first call of the per-mode screen function): does that call,
@ then, in mode 0x40, draws the list. Stock draws nothing for an unknown mode, so nothing else is on the screen.
        .global cc_draw
        .thumb_func
cc_draw:
        push    {r4, r5, r6, r7, lr}
        bl      DRAW_PREP
        ldr     r0, =MODE
        ldrb    r0, [r0]
        cmp     r0, #MODE_PICK
        bne     39f
        ldr     r0, =FONT_TITLE
        bl      SET_FONT
        adr     r2, s_title
        movs    r1, #0x0e
        movs    r0, #0
        bl      DRAW_STR
        ldr     r0, =FONT_ROW
        bl      SET_FONT
        bl      cc_list
        mov     r7, r0                  @ r7 = the list
        ldr     r4, =CC_STATE
        ldrb    r4, [r4, #5]            @ r4 = cursor
        ldrb    r3, [r7, #4]            @ r3 = count
        subs    r5, r4, #2              @ r5 = first row shown, keeping the cursor in the middle
        bpl     30f
        movs    r5, #0
30:     subs    r0, r3, #5              @ the last first-row: count - 5 (0 for a short list)
        bpl     31f
        movs    r0, #0
31:     cmp     r5, r0
        ble     32f
        mov     r5, r0
32:     movs    r6, #0                  @ row 0 to 4
33:     adds    r0, r5, r6              @ index
        ldrb    r3, [r7, #4]
        cmp     r0, r3
        bhs     36f                     @ past the last item: no more rows
        movs    r2, #NAME_BYTES
        muls    r2, r0, r2
        adds    r2, r2, r7
        adds    r2, #8                  @ r2 = this name
        lsls    r1, r6, #4
        adds    r1, #0x1e               @ y
        push    {r0, r1}
        movs    r0, #0x14
        bl      DRAW_STR
        pop     {r0, r1}
        cmp     r0, r4
        bne     34f
        adr     r2, s_arrow
        movs    r0, #5
        bl      DRAW_STR
34:     adds    r6, #1
        cmp     r6, #5
        blt     33b
36:     adr     r2, s_help
        movs    r1, #0x71
        movs    r0, #0
        bl      DRAW_STR
39:     pop     {r4, r5, r6, r7, pc}


@ ---------------------------------------------------------------- a pick rides the record as it is BUILT (build 10)
@ A normal shot (DIST) builds its record into the buffer at 0x20000c44 and sends it to the phone from there, before the
@ store loop runs: cc_store alone left the pick out of the live packet (only the stored copy had it). So the pick is also
@ put into the record right after it is built. cc_build replaces "bl 0x0800802c" (the record builder) at 0x0800b62e,
@ 0x0800b674 and 0x0800b74e: it remembers the record and the caller's return address, runs the builder with itself as the
@ return address, then adds the pick (once) and goes back. RAM: CC_STATE +0x10 the return address, +0x14 the record.
        .global cc_build
        .thumb_func
cc_build:
        ldr     r12, =CC_STATE
        str     lr, [r12, #0x10]
        str     r0, [r12, #0x14]
        ldr     lr, =cc_build_done
        b.w     BUILD_REC

        .thumb_func
cc_build_done:
        ldr     r12, =CC_STATE
        ldrb    r1, [r12]
        cmp     r1, #CC_MAGIC
        bne     79f
        ldrb    r1, [r12, #6]
        cbz     r1, 79f
        ldr     r2, [r12, #0x14]
        strb    r1, [r2, #54]
        movs    r1, #0xca
        strb    r1, [r2, #55]
        movs    r1, #0
        strb    r1, [r12, #6]           @ a pick is used once
79:     ldr     lr, [r12, #0x10]
        bx      lr

@ ---------------------------------------------------------------- the list: where it lives, how it is written (cc8)
@ cc_list: r0 = the list in use: the newest valid slot in flash, else callout_block. Cached in CC_STATE +8.
        .global cc_list
        .thumb_func
cc_list:
        push    {r4, lr}
        bl      cc_state_init
        ldr     r4, =CC_STATE
        ldr     r0, [r4, #8]
        cbnz    r0, 81f
        bl      cc_scan
        cbnz    r0, 80f
        adr     r0, callout_block
80:     str     r0, [r4, #8]
81:     pop     {r4, pc}

@ cc_scan: r0 = the newest valid slot (0 if none), r1 = the first empty slot (FLASH_LIST_END if the sector is full).
        .thumb_func
cc_scan:
        push    {r4, r5, r6, lr}
        ldr     r4, =FLASH_LIST
        ldr     r5, =FLASH_LIST_END
        mov     r6, r4
82:     cmp     r6, r5
        bhs     83f
        ldr     r0, [r6]
        adds    r0, #1                  @ 0xffffffff: erased, nothing was ever written here
        beq     83f
        add     r6, r6, #SLOT
        b       82b
83:     mov     r5, r6                  @ r5 = first empty
84:     cmp     r6, r4                  @ walk back to the newest slot that checks out
        bls     86f
        sub     r6, r6, #SLOT
        mov     r0, r6
        bl      cc_valid
        cmp     r0, #0
        beq     84b
        mov     r0, r6
        mov     r1, r5
        pop     {r4, r5, r6, pc}
86:     movs    r0, #0
        mov     r1, r5
        pop     {r4, r5, r6, pc}

@ cc_valid: r0 = a block; r0 = 1 if it is "CCAL", a count of 32 or less, and its CRC checks, else 0.
        .thumb_func
cc_valid:
        push    {r4, lr}
        mov     r4, r0
        ldr     r1, [r4]
        movw    r2, #0x4343             @ "CCAL", built from two halves so the bytes C C A L appear only in the data block
        movt    r2, #0x4c41
        cmp     r1, r2
        bne     87f
        ldrb    r1, [r4, #4]
        cmp     r1, #32
        bhi     87f
        mov     r0, r4
        movw    r1, #BLOCK_LEN
        bl      cc_crc16
        ldrh    r1, [r4, #BLOCK_LEN]
        cmp     r0, r1
        bne     87f
        movs    r0, #1
        pop     {r4, pc}
87:     movs    r0, #0
        pop     {r4, pc}

@ cc_crc16: CRC-16/Modbus of r1 bytes at r0 (the one the bootloader's packets use), in r0.
        .thumb_func
cc_crc16:
        movw    r2, #0xffff
        movw    r12, #0xa001
88:     cbz     r1, 91f
        ldrb    r3, [r0], #1
        eors    r2, r2, r3
        movs    r3, #8
89:     lsrs    r2, r2, #1
        bcc     90f
        eor.w   r2, r2, r12
90:     subs    r3, #1
        bne     89b
        subs    r1, #1
        b       88b
91:     mov     r0, r2
        bx      lr

@ cc_addr replaces "bl 0x08008594" (the memory-address-to-pointer map) at 0x0800c420 and 0x0800c450, inside the
@ 3d/3e handler: addresses 0xC000..0xC20F are ours (the buffer, then the result block) and 0xC400..0xC5A9 shows the
@ list in use (read only); the rest is stock.
        .global cc_addr
        .thumb_func
cc_addr:
        movw    r2, #LIST_ADDR
        subs    r2, r0, r2
        cmp     r2, #LIST_SPAN
        bhs     92f
        adds    r3, r2, r1
        cmp     r3, #LIST_SPAN
        bhi     94f                     @ runs past the end: 0 = refused
        ldr     r0, =CC_LISTBUF
        adds    r0, r0, r2              @ the buffer and the result block are one run of RAM
        bx      lr
94:     movs    r0, #0
        bx      lr
92:     movw    r2, #LIST_READ          @ cc9: the list in use, read-only
        subs    r2, r0, r2
        cmp     r2, #SLOT_LEN
        bhs     95f
        adds    r3, r2, r1
        cmp     r3, #SLOT_LEN
        bhi     94b
        push    {r2, lr}
        bl      cc_list
        pop     {r2, lr}
        adds    r0, r0, r2
        bx      lr
95:     b.w     STOCK_VALIDATE

@ cc_write replaces "bl 0x080132d0" (the memory write) at 0x0800c448. r0 = address, r1 = data, r2 = count.
@ Bytes for 0xC000..0xC1FF fill the buffer; a write whose first byte is 0xC0 to 0xC200 commits it.
        .global cc_write
        .thumb_func
cc_write:
        movw    r3, #LIST_ADDR
        subs    r3, r0, r3
        cmp     r3, #LIST_SPAN
        bhs     99f
        push    {r4, lr}
        cmp     r3, #0x200
        bhs     97f
        adds    r0, r3, r2
        cmp     r0, #0x200
        bhi     98f                     @ past the buffer: ignored
        ldr     r0, =CC_LISTBUF
        adds    r0, r0, r3
96:     cbz     r2, 98f
        ldrb    r4, [r1], #1
        strb    r4, [r0], #1
        subs    r2, #1
        b       96b
97:     ldrb    r4, [r1]
        cmp     r4, #0xc0
        bne     98f
        bl      cc_commit
98:     pop     {r4, pc}
99:     b.w     STOCK_WRITE

@ cc_commit: checks the buffer and programs it into the next empty slot. The result goes to CC_STATUS.
        .thumb_func
cc_commit:
        push    {r4, r5, r6, lr}
        ldr     r4, =CC_STATUS
        movs    r0, #0xff
        strb    r0, [r4]                @ 0xff: not finished
        ldr     r5, =CC_LISTBUF
        mov     r0, r5
        bl      cc_valid
        cbnz    r0, 100f
        movs    r0, #2
        b       109f
100:    bl      cc_scan
        mov     r6, r1                  @ the slot to use
        ldr     r0, =FLASH_LIST_END
        cmp     r6, r0
        blo     101f
        movs    r0, #3
        b       109f
101:    mov     r1, r6                  @ the whole slot must read erased (all 0xff)
        movs    r2, #SLOT / 4
102:    ldr     r0, [r1], #4
        adds    r0, #1
        bne     103f
        subs    r2, #1
        bne     102b
        b       104f
103:    movs    r0, #4
        b       109f
104:    mov     r0, r6
        movw    r1, #SLOT_LEN
        mov     r2, r5
        bl      FLASH_WRITE_HALF
        cmp     r0, #1
        beq     105f
        movs    r0, #5
        b       109f
105:    mov     r1, r6                  @ read it back
        mov     r2, r5
        movw    r3, #SLOT_LEN
106:    ldrb    r0, [r1], #1
        ldrb    r12, [r2], #1
        cmp     r0, r12
        bne     107f
        subs    r3, #1
        bne     106b
        b       108f
107:    movs    r0, #5
        b       109f
108:    ldr     r1, =CC_STATE
        movs    r0, #0
        str     r0, [r1, #8]            @ the picker looks the list up again
        ldr     r0, =FLASH_LIST
        subs    r0, r6, r0
        lsrs    r0, r0, #9
        strb    r0, [r4, #1]            @ the slot used
        ldrb    r0, [r5, #4]
        strb    r0, [r4, #2]            @ names in it
        movs    r0, #0
109:    strb    r0, [r4]
        movs    r0, #CC_VERSION
        strb    r0, [r4, #3]
        pop     {r4, r5, r6, pc}

        .ltorg

s_title:  .asciz "Feature"
s_arrow:  .asciz ">"
s_help:   .asciz "M:Pick CLR:Back"

@ The list. x1flash finds it by its first four bytes ("CCAL") and rewrites count and names; keep this layout:
@   +0  "CCAL"   +4 count (0 to 32)   +5..7 zero   +8 name 0, name 1, ... each 13 bytes: up to 12 ASCII characters,
@   then zeros (always a terminating 0). Room for 32 names.
        .balign 4
callout_block:
        .ascii  "CCAL"
        .byte   16, 0, 0, 0
        .macro  CNAME text
1:      .asciz  "\text"
2:      .space  NAME_BYTES - (2b - 1b)
        .endm
        CNAME   "Lead"
        CNAME   "Squeeze"
        CNAME   "Drip"
        CNAME   "Stream"
        CNAME   "Pool"
        CNAME   "Sump"
        CNAME   "Drop"
        CNAME   "Climb"
        CNAME   "Crawl"
        CNAME   "Breakdown"
        CNAME   "Flowstone"
        CNAME   "Column"
        CNAME   "Straw"
        CNAME   "Bats"
        CNAME   "Bones"
        CNAME   "Hazard"
        .space  NAME_BYTES * (MAX_NAMES - 16)
        .balign 4
        .ltorg
