"""Decodes an Apple PacketLogger capture (.pklg, iOS trace) of an Osmo gimbal and an app into DJI DUML messages.

    python3 -I tools/cave-osmo/capture/decode_pklg.py capture.pklg [--all]

Prints a count of every message kind (direction, sender, receiver, set/cmd, flags) and, with --all, every message
with its time. PacketLogger records: 4-byte length + 8-byte time (low 32 bits seconds, high 32 microseconds) + type
(2 = ACL sent, 3 = ACL received) + HCI ACL. ATT write-without-response (0x52) and notification (0x1b) payloads are
joined per direction and cut into DUML messages; checksum: CRC-8 seed 0x77 on the header, CRC-16 seed 0x3692 over the
whole message (FINDINGS.md)."""
import collections, struct, sys

def tables():
    t8, t16 = [], []
    for i in range(256):
        c = i
        for _ in range(8): c = (c >> 1) ^ 0x8C if c & 1 else c >> 1
        t8.append(c); c = i
        for _ in range(8): c = (c >> 1) ^ 0x8408 if c & 1 else c >> 1
        t16.append(c)
    return t8, t16
T8, T16 = tables()
def crc8(d, c=0x77):
    for b in d: c = T8[(b ^ c) & 0xFF]
    return c
def crc16(d, c=0x3692):
    for b in d: c = (c >> 8) ^ T16[(b ^ c) & 0xFF]
    return c

def records(data):
    i = 0
    while i + 13 <= len(data):
        n = struct.unpack('<I', data[i:i + 4])[0]
        if n < 9 or i + 4 + n > len(data): return
        ts = struct.unpack('<Q', data[i + 4:i + 12])[0]
        yield (ts & 0xFFFFFFFF) + (ts >> 32) / 1e6, data[i + 12], data[i + 13:i + 4 + n]
        i += 4 + n

def att(data):
    frag = {}
    for t, typ, p in records(data):
        if typ not in (2, 3) or len(p) < 4: continue
        h, ln = struct.unpack('<HH', p[:4]); body = p[4:4 + ln]
        if (h >> 12) & 3 == 1:
            if typ not in frag: continue
            frag[typ] += body
        else:
            frag[typ] = body
        l2 = frag[typ]
        if len(l2) < 4: continue
        l2len, cid = struct.unpack('<HH', l2[:4])
        if len(l2) - 4 < l2len: continue
        del frag[typ]
        if cid == 4: yield t, 'tx' if typ == 2 else 'rx', l2[4:4 + l2len]

def messages(data):
    buf = {'tx': b'', 'rx': b''}; t0 = None
    for t, d, a in att(data):
        t0 = t if t0 is None else t0
        if a[0] not in (0x52, 0x1B): continue
        buf[d] += a[3:]; b = buf[d]
        while True:
            i = b.find(b'\x55')
            if i < 0: b = b''; break
            b = b[i:]
            if len(b) < 4: break
            ln = (b[1] | b[2] << 8) & 0x3FF
            if ln < 13 or crc8(b[:3]) != b[3]: b = b[1:]; continue
            if len(b) < ln: break
            raw, b = b[:ln], b[ln:]
            if crc16(raw[:-2]) != (raw[-2] | raw[-1] << 8): continue
            yield dict(t=t - t0, d=d, snd=raw[4], rcv=raw[5], seq=raw[6] | raw[7] << 8, attr=raw[8], set=raw[9], cmd=raw[10], p=raw[11:-2])
        buf[d] = b

if __name__ == '__main__':
    ms = list(messages(open(sys.argv[1], 'rb').read()))
    print(f'{len(ms)} messages')
    if '--all' in sys.argv:
        for m in ms: print(f"{m['t']:8.2f} {m['d']} {m['snd']:02x}->{m['rcv']:02x} {m['set']:02x}/{m['cmd']:02x} a={m['attr']:02x} #{m['seq']} [{m['p'].hex(' ')}]")
    else:
        c = collections.Counter((m['d'], m['snd'], m['rcv'], m['set'], m['cmd'], m['attr']) for m in ms)
        for k, v in sorted(c.items(), key=lambda kv: (kv[0][0], -kv[1])): print(f'{k[0]} {k[1]:02x}->{k[2]:02x} {k[3]:02x}/{k[4]:02x} a={k[5]:02x} x{v}')
