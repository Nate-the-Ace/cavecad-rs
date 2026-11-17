import struct,sys
# targets as yaw:roll:pitch ; prints probe steps: enter, upload, go to each, exit
pts=[tuple(float(v) for v in a.split(":")) for a in sys.argv[2:]]
spd=int(__import__("os").environ.get("WP_SPEED","60"))
wait=sys.argv[1]
body=bytes([0x2c,0,0,0x0a,len(pts)])
for y,r,p in pts: body+=struct.pack("<Hhhh",spd,round(y*10),round(r*10),round(p*10))
body+=bytes([0x56])
pay=bytes([1,len(body)])+body
steps=['hello','wait:2500','raw:04:4c:0100:40','raw:04:0f:220100:40','raw:04:6b:0000006d:40',f'raw:04:6d:{pay.hex()}:40',f'wait:{wait}']
for i in range(1,len(pts)): steps+= [f'raw:04:6d:00064000{i:02x}{int(__import__("os").environ.get("WP_STEP","10")):02x}0000:40',f'wait:{wait}']
steps+=['raw:04:6b:0001006d:40','raw:04:6d:0106c00000000000:40','wait:2500']
print(' '.join(steps))
