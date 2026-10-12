# SPDX-License-Identifier: GPL-3.0-only
# Dump world-processor code from a .msav (G06 audit helper). Usage: python3 -I dumpcode.py <map>.msav
import sys,zlib,struct
p=sys.argv[1]
data=zlib.decompress(open(p,'rb').read())
i=data.find(b'\x78\x9c'); n=0
while i!=-1:
  try:
    out=zlib.decompressobj().decompress(data[i:i+40000])
    if len(out)>5 and out[0] in (0,1):
      ln=struct.unpack('>i',out[1:5])[0]
      if 0<ln<len(out):
        code=out[5:5+ln].decode('utf8','replace')
        if any(k in code for k in ('getflag','setrule','sensor','wait','fetch','cutscene','spawn','ubind')):
          n+=1; print('---- block',n,'len',ln); print(code[:3000])
  except Exception: pass
  i=data.find(b'\x78\x9c',i+2)
