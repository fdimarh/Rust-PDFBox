import re, struct, subprocess, tempfile, os

data = open('/Users/fdimarh/Downloads/2024sk-kma260_KMA_SK.KP5_XII_2024.pdf', 'rb').read()

def extract_jbig2_stream(data, obj_num):
    for pat in [('\n%d 0 obj' % obj_num).encode(), ('\r%d 0 obj' % obj_num).encode()]:
        pos = data.find(pat)
        if pos >= 0:
            break
    if pos < 0:
        return None, 0, 0
    chunk = data[pos:pos+500]
    ln_m = re.search(rb'/Length\s+(\d+)', chunk)
    w_m = re.search(rb'/Width\s+(\d+)', chunk)
    h_m = re.search(rb'/Height\s+(\d+)', chunk)
    length = int(ln_m.group(1)) if ln_m else 0
    w = int(w_m.group(1)) if w_m else 0
    h = int(h_m.group(1)) if h_m else 0
    kw_pos = chunk.find(b'stream')
    abs_pos = pos + kw_pos + 6
    if data[abs_pos] == ord('\r'): abs_pos += 1
    if data[abs_pos] == ord('\n'): abs_pos += 1
    return data[abs_pos:abs_pos+length], w, h

# Test with obj 213
raw, w, h = extract_jbig2_stream(data, 213)
print("JBIG2 stream: %d bytes, %dx%d" % (len(raw), w, h))

# Write raw stream
with tempfile.NamedTemporaryFile(suffix='.jbig2', delete=False) as f:
    f.write(raw)
    jbig2_path = f.name

out_pbm = jbig2_path + '.pbm'

# Use --embedded flag for PDF JBIG2 streams
result = subprocess.run(
    ['/usr/local/bin/jbig2dec', '--embedded', '-t', 'pbm', '-o', out_pbm, jbig2_path],
    capture_output=True
)
print("jbig2dec exit: %d" % result.returncode)
print("stderr: %r" % result.stderr[:200])

if os.path.exists(out_pbm):
    pbm_size = os.path.getsize(out_pbm)
    print("PBM output: %d bytes (%.1f MB)" % (pbm_size, pbm_size/1048576))
    # PBM format: P4\n<W> <H>\n<binary data>
    with open(out_pbm, 'rb') as f:
        header = f.readline()  # P4
        dims = f.readline()    # W H
        print("PBM header: %r %r" % (header, dims))
    os.unlink(out_pbm)
else:
    print("No PBM output")

os.unlink(jbig2_path)

