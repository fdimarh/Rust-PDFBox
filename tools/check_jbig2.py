import re, os

data = open('/Users/fdimarh/Downloads/2024sk-kma260_KMA_SK.KP5_XII_2024.pdf', 'rb').read()

# Find JBIG2 images and check for JBIG2Globals
print("=== JBIG2 image objects ===")
count = 0
for m in re.finditer(rb'/Filter\s*/JBIG2Decode', data):
    p = m.start()
    before = data[max(0,p-300):p]
    obj_m = list(re.finditer(rb'(\d+)\s+(\d+)\s+obj', before))
    if not obj_m:
        continue
    lm = obj_m[-1]
    obj_num = int(lm.group(1))
    chunk = data[max(0,p-300):p+500]

    ln = re.search(rb'/Length\s+(\d+)', chunk)
    w = re.search(rb'/Width\s+(\d+)', chunk)
    h = re.search(rb'/Height\s+(\d+)', chunk)
    bpc = re.search(rb'/BitsPerComponent\s+(\d+)', chunk)
    # Check for JBIG2Globals
    glob = re.search(rb'/DecodeParms\s*<<([^>]+)>>', chunk)
    glob2 = re.search(rb'/JBIG2Globals\s+(\d+)\s+(\d+)\s+R', chunk)

    count += 1
    if count <= 5:
        print("  obj %d: w=%s h=%s bpc=%s len=%s globals=%s" % (
            obj_num,
            w.group(1).decode() if w else '?',
            h.group(1).decode() if h else '?',
            bpc.group(1).decode() if bpc else '?',
            ln.group(1).decode() if ln else '?',
            ('obj %s %s R' % (glob2.group(1).decode(), glob2.group(2).decode())) if glob2 else 'none'
        ))

print("  ... total JBIG2 objects: %d" % count)

# Check all JBIG2Globals references
print()
print("=== JBIG2Globals references ===")
globals_refs = set()
for m in re.finditer(rb'/JBIG2Globals\s+(\d+)\s+(\d+)\s+R', data):
    ref = (int(m.group(1)), int(m.group(2)))
    globals_refs.add(ref)
print("  Globals objects: %s" % sorted(globals_refs))

# Find the globals object content
for (gnum, ggen) in sorted(globals_refs):
    pattern = ('\n%d %d obj' % (gnum, ggen)).encode()
    pos = data.find(pattern)
    if pos < 0:
        pattern = ('\r%d %d obj' % (gnum, ggen)).encode()
        pos = data.find(pattern)
    if pos >= 0:
        chunk = data[pos:pos+300]
        ln = re.search(rb'/Length\s+(\d+)', chunk)
        print("  JBIG2Globals obj %d: len=%s" % (gnum, ln.group(1).decode() if ln else '?'))
        print("  dict: %r" % chunk[:150])

# Check what jbig2dec CLI can do
print()
print("=== Testing jbig2dec CLI ===")
import subprocess
r = subprocess.run(['/usr/local/bin/jbig2dec', '--version'], capture_output=True)
print("  version output: %r" % (r.stdout[:100] + r.stderr[:100]))
print("  help modes: ", end='')
r2 = subprocess.run(['/usr/local/bin/jbig2dec', '--help'], capture_output=True)
lines = (r2.stdout + r2.stderr).decode('latin1', errors='replace').splitlines()
for l in lines[:5]:
    print(l)

