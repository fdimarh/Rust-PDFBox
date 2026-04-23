import re, struct, subprocess, tempfile, os

data = open('/Users/fdimarh/Downloads/2024sk-kma260_KMA_SK.KP5_XII_2024.pdf', 'rb').read()

# Extract the first JBIG2 stream (obj 213)
target_obj = 213
pattern = ('\n%d 0 obj' % target_obj).encode()
pos = data.find(pattern)
if pos < 0:
    pattern = ('\r%d 0 obj' % target_obj).encode()
    pos = data.find(pattern)

print("Object %d found at offset %d" % (target_obj, pos))
chunk = data[pos:pos+500]
print("Dict: %r" % chunk[:200])

# Get /Length
ln_m = re.search(rb'/Length\s+(\d+)', chunk)
length = int(ln_m.group(1))
print("Length: %d" % length)

# Find stream start
stream_kw = b'stream'
kw_pos = chunk.find(stream_kw)
abs_stream_pos = pos + kw_pos + len(stream_kw)
# Skip \r\n or \n after 'stream'
if data[abs_stream_pos] == ord('\r'):
    abs_stream_pos += 1
if data[abs_stream_pos] == ord('\n'):
    abs_stream_pos += 1

raw_jbig2 = data[abs_stream_pos:abs_stream_pos+length]
print("Extracted %d bytes of JBIG2 data" % len(raw_jbig2))

# For PDF JBIG2 embedded streams (no globals), we need to prepend
# a JBIG2 file header to make it a valid standalone file.
# PDF JBIG2 streams are "page stream" format without the file header.
# jbig2dec needs the full file or (globals, page) pair.
#
# The JBIG2 file header: 0x97 0x4A 0x42 0x32 0x0D 0x0A 0x1A 0x0A
# Then segment headers follow directly.
# For embedded PDF streams without globals: prepend the header.

JBIG2_FILE_HEADER = bytes([0x97, 0x4A, 0x42, 0x32, 0x0D, 0x0A, 0x1A, 0x0A])

# Check if the raw data already starts with the file header
if raw_jbig2[:8] == JBIG2_FILE_HEADER:
    full_jbig2 = raw_jbig2
    print("Has file header already")
else:
    print("No file header, prepending...")
    full_jbig2 = JBIG2_FILE_HEADER + raw_jbig2

# Write to temp file
with tempfile.NamedTemporaryFile(suffix='.jbig2', delete=False) as f:
    f.write(full_jbig2)
    jbig2_path = f.name

out_path = jbig2_path + '.png'

# Run jbig2dec to decode to PNG
result = subprocess.run(
    ['/usr/local/bin/jbig2dec', '-t', 'png', '-o', out_path, jbig2_path],
    capture_output=True
)
print("jbig2dec stdout: %r" % result.stdout[:200])
print("jbig2dec stderr: %r" % result.stderr[:200])
print("jbig2dec return code: %d" % result.returncode)

if os.path.exists(out_path):
    print("Output PNG: %d bytes" % os.path.getsize(out_path))
else:
    print("No output PNG generated")

os.unlink(jbig2_path)
if os.path.exists(out_path):
    os.unlink(out_path)

