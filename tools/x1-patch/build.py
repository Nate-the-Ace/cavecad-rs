#!/usr/bin/env python3
"""Builds CVWY_X1_1.1.7-cc4.bin: Cavway's public 1.1.7 plus CaveCAD scan control (cc.s).

    python3 tools/x1-patch/build.py [path/to/CVWY_X1_1.1.7.bin]

Refuses any base file but the exact public 1.1.7 (by sha256) and checks every original instruction it replaces,
so a patch can never land on code it was not written for. Needs only Xcode's clang (it assembles Thumb into an
ELF object; this script links that object by hand: places it, fixes its branches, splices the hooks in).
Writes build/CVWY_X1_1.1.7-cc4.bin, which X1 Flasher (and x1flash) accept like any Cavway file.
"""
import hashlib
import os
import struct
import subprocess
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
BASE_SHA = "18060d77000d45ddbac0369034ec9400646cde7e56d35adfc32cbe90dfaf5d51"
DEFAULT_BASE = os.path.expanduser("~/Documents/github/cavway-firmware/CVWY_X1_1.1.7.bin")
APP = 0x08004000
HEADER = 256
PATCH_BASE = 0x0801D200  # free flash; the stock image ends at 0x0801D138
APP_LIMIT = 0x0803FF00   # the app's own marker word lives here
NAME = "1.1.7-cc9"

# (address, original bytes, what goes there)
HOOKS = [
    (0x0800C3DE, "352c5bd1", ("b.w", "cc_dispatch")),   # cmp r4,#0x35 / bne exit
    (0x0800D500, "0020ca49", ("bl", "cc_scan_end")),    # movs r0,#0 / ldr r1,=SCAN_TICKS
    (0x080132C4, "00f0c0f8", ("bl", "cc_store")),       # bl STORE_RECORD
    (0x0800641A, "1e480068", ("bl", "cc_usb_gate")),    # ldr r0,=USB_OK / ldr r0,[r0] (USB sender gate)
    (0x0800C682, "fff74dff", ("bl", "cc_defaults")),    # bl SET_DEFAULTS_2 (factory defaults)
    (0x08015BC0, "06480078", ("bl", "cc_settings_boot")),  # ldr r0,=0x200010dc / ldrb r0,[r0] (after settings load)
    (0x0800CFE0, "10b58ab0", ("b.w", "cc_ui")),         # push {r4,lr} / sub sp,#0x28 (UI state machine entry; cc_ui redoes them)
    (0x080072BA, "fff739f9", ("bl", "cc_draw")),
    (0x0800791A, "0bf095fd", ("bl", "cc_store")),
    (0x0800C420, "fcf7b8f8", ("bl", "cc_addr")),        # bl 0x08008594 (memory address to pointer), in the 3d/3e handler
    (0x0800C448, "06f042ff", ("bl", "cc_write")),       # bl 0x080132d0 (the memory write) in the 3e handler
    (0x0800C450, "fcf7a0f8", ("bl", "cc_addr")),        # the same map, for the reply       # bl STORE_RECORD in the store-N-records loop a normal shot (DIST) uses        # bl DRAW_PREP (first call of the per-mode screen drawing)
]
STOCK = {"DISP_MEASURE": 0x0800C3E2, "DISP_EXIT": 0x0800C49A, "BEEP": 0x0800CA40, "LASER_CONT": 0x08016098,
         "STORE_RECORD": 0x08013448, "SET_DEFAULTS_2": 0x0800C520,
         "SAVE_SETTINGS": 0x0800C88C, "UI_CONT": 0x0800CFE4, "UI_EXIT": 0x0800E7C8, "DRAW_PREP": 0x08006530,
         "SET_FONT": 0x0800CF88, "DRAW_STR": 0x080078F0, "STOCK_VALIDATE": 0x08008594, "STOCK_WRITE": 0x080132D0,
         "FLASH_WRITE_HALF": 0x08007DE0}
VERSION_TEXT = 0x08017B84  # "1.1.7" + 5 NULs, copied to the Information page (10 bytes free before the next table)


def thumb_branch(at, target, link):
    off = target - (at + 4)
    assert -(1 << 24) <= off < (1 << 24) and off % 2 == 0, f"branch out of range {at:#x} -> {target:#x}"
    s = (off >> 24) & 1
    i1, i2 = (off >> 23) & 1, (off >> 22) & 1
    j1, j2 = (~i1 ^ s) & 1, (~i2 ^ s) & 1
    hi = 0xF000 | (s << 10) | ((off >> 12) & 0x3FF)
    lo = (0xD000 if link else 0x9000) | (j1 << 13) | (j2 << 11) | ((off >> 1) & 0x7FF)
    return struct.pack("<HH", hi, lo)


def thumb_cond_branch_w(at, target, cond):
    off = target - (at + 4)
    assert -(1 << 20) <= off < (1 << 20)
    s = (off >> 20) & 1
    j2, j1 = (off >> 19) & 1, (off >> 18) & 1
    hi = 0xF000 | (s << 10) | (cond << 6) | ((off >> 12) & 0x3F)
    lo = 0x8000 | (j1 << 13) | (j2 << 11) | ((off >> 1) & 0x7FF)
    return struct.pack("<HH", hi, lo)


def assemble(src):
    obj = os.path.join(HERE, "build", "cc.o")
    os.makedirs(os.path.dirname(obj), exist_ok=True)
    subprocess.run(["clang", "-target", "thumbv7em-none-eabi", "-mcpu=cortex-m4", "-c", src, "-o", obj], check=True)
    return open(obj, "rb").read()


def link(elf, base):
    """Places .text at `base`; resolves its branches and literals. Returns (code, {global symbol: address})."""
    shoff, = struct.unpack_from("<I", elf, 0x20)
    shentsize, shnum, shstrndx = struct.unpack_from("<HHH", elf, 0x2E)
    secs = [struct.unpack_from("<IIIIIIIIII", elf, shoff + i * shentsize) for i in range(shnum)]
    names = secs[shstrndx]
    name = lambda off, tab: elf[tab[4] + off: elf.index(b"\0", tab[4] + off)].decode()
    by = {name(s[0], names): (i, s) for i, s in enumerate(secs)}
    ti, text = by[".text"]
    code = bytearray(elf[text[4]: text[4] + text[5]])
    _, symtab = by[".symtab"]
    strtab = secs[symtab[6]]
    syms = []
    for k in range(symtab[5] // 16):
        st_name, st_value, st_size, st_info, st_other, st_shndx = struct.unpack_from("<IIIBBH", elf, symtab[4] + k * 16)
        n = name(st_name, strtab) if st_name else ""
        if st_shndx == 0xFFF1:          # absolute (.set)
            addr = st_value
        elif st_shndx == ti:
            addr = base + (st_value & ~1)
        else:
            addr = STOCK.get(n)
        syms.append((n, addr, st_info >> 4))
    exports = {n: a for n, a, bind in syms if bind == 1 and a is not None}
    if ".rel.text" in by:
        _, rel = by[".rel.text"]
        for k in range(rel[5] // 8):
            r_off, r_info = struct.unpack_from("<II", elf, rel[4] + k * 8)
            n, target, _ = syms[r_info >> 8]
            typ = r_info & 0xFF
            assert target is not None, f"unresolved {n}"
            at = base + r_off
            if typ in (10, 30):  # R_ARM_THM_CALL, R_ARM_THM_JUMP24
                link_bit = typ == 10 and (struct.unpack_from("<H", code, r_off + 2)[0] & 0x1000) == 0x1000 and \
                    (struct.unpack_from("<H", code, r_off + 2)[0] & 0xD000) == 0xD000
                code[r_off: r_off + 4] = thumb_branch(at, target, link_bit)
            elif typ == 51:  # R_ARM_THM_JUMP19
                cond = (struct.unpack_from("<H", code, r_off)[0] >> 6) & 0xF
                code[r_off: r_off + 4] = thumb_cond_branch_w(at, target, cond)
            elif typ == 2:   # R_ARM_ABS32
                addend, = struct.unpack_from("<I", code, r_off)
                code[r_off: r_off + 4] = struct.pack("<I", (target + addend) & 0xFFFFFFFF)
            else:
                sys.exit(f"relocation type {typ} for {n} not handled")
    return bytes(code), exports


def main():
    base_path = sys.argv[1] if len(sys.argv) > 1 else DEFAULT_BASE
    f = open(base_path, "rb").read()
    sha = hashlib.sha256(f).hexdigest()
    if sha != BASE_SHA:
        sys.exit(f"{base_path} is not Cavway's public 1.1.7 (sha256 {sha[:16]}…); refusing to patch")
    payload = bytearray(f[HEADER:])
    end = APP + len(payload)
    assert end <= PATCH_BASE, "stock image grew into the patch area"

    code, sym = link(assemble(os.path.join(HERE, "cc.s")), PATCH_BASE)
    assert PATCH_BASE + len(code) <= APP_LIMIT
    for at, orig, (kind, target) in HOOKS:
        o = at - APP
        have = payload[o: o + 4].hex()
        if have != orig:
            sys.exit(f"{at:#x}: expected {orig}, found {have}; refusing")
        payload[o: o + 4] = thumb_branch(at, sym[target], kind == "bl")

    # the name: Information page string, and the version block (executed at boot, so only harmless Thumb):
    # 01 01 07 00 | cc 20 = movs r0,#0xcc | nn 20 = movs r0,#nn  ->  "1.1.7", "cc", patch nn
    o = VERSION_TEXT - APP
    assert payload[o: o + 10] == b"1.1.7" + b"\0" * 5
    payload[o: o + 10] = NAME.encode() + b"\0"
    assert payload[4:8] == b"\0" * 4
    payload[4:8] = bytes([0xCC, 0x20, int(NAME.rsplit("cc", 1)[1]), 0x20])

    # the patch code, after a gap of erased flash
    payload += b"\xff" * (PATCH_BASE - end) + code

    header = bytearray(f[:HEADER])
    header[8:12] = struct.pack(">I", int(time.time()))
    out = os.path.join(HERE, "build", f"CVWY_X1_{NAME}.bin")
    open(out, "wb").write(bytes(header) + bytes(payload))
    print(f"{out}\n  patch code {PATCH_BASE:#x}..{PATCH_BASE + len(code):#x} ({len(code)} bytes)")
    for n in ("cc_dispatch", "cc_scan_end", "cc_store", "cc_usb_gate", "cc_defaults", "cc_settings_boot", "cc_ui", "cc_draw", "cc_addr", "cc_write", "cc_list"):
        print(f"  {n:12s} {sym[n]:#x}")
    print(f"  sha256 {hashlib.sha256(open(out, 'rb').read()).hexdigest()}")


if __name__ == "__main__":
    main()
