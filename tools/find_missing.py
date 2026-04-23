import re, struct, os

data = open('/Users/fdimarh/Downloads/2024sk-kma260_KMA_SK.KP5_XII_2024.pdf', 'rb').read()

# Find exactly where objects 211, 279, 285 are in the file
targets = [211, 279, 285]

print("=== Searching for objects 211, 279, 285 ===")
for t in targets:
    needle = ("\n%d 0 obj" % t).encode()
    pos = data.find(needle)
    if pos < 0:
        needle2 = ("\r%d 0 obj" % t).encode()
        pos = data.find(needle2)
    print("Object %d: offset=%d" % (t, pos))
    if pos >= 0:
        chunk = data[pos:pos+300]
        print("  dict: %r" % chunk[:150])
        # Find /Length
        lm = re.search(rb'/Length\s+(\d+)', chunk)
        print("  /Length: %s" % (lm.group(1).decode() if lm else 'not found'))
        # Check byte 143 in the slice
        slice_start = pos + 1  # skip leading \n
        if len(data) > slice_start + 150:
            b143 = data[slice_start + 143]
            print("  byte at offset 143 from obj start: 0x%02X (%r)" % (b143, bytes([b143])))

# Also check what the linear scan's try_parse_obj_header would find
# The linear scanner starts from the digit, not from \n
print()
print("=== Verifying linear scan finds these objects ===")
for t in targets:
    pattern = ("%d 0 obj" % t).encode()
    for m in re.finditer(re.escape(pattern), data):
        p = m.start()
        print("  '%s' found at offset %d" % (pattern.decode(), p))
        # Check what's before it - is it a newline/CR?
        if p > 0:
            prev = data[p-1]
            print("    prev byte: 0x%02X (%r)" % (prev, bytes([prev])))
        break

# Check what "parse error at byte 143" means - the parser reads slice from object start
# slice = &bytes[offset..] where offset is where linear scan found "N G obj"
# "byte 143" in slice means 143 bytes after "N G obj"
print()
print("=== Content at byte 143 after 'N 0 obj' ===")
for t in targets:
    pattern = ("%d 0 obj" % t).encode()
    m = re.search(re.escape(pattern), data)
    if m:
        body_start = m.end()  # right after "N G obj"
        slice_ = data[body_start:]
        b143 = slice_[143] if len(slice_) > 143 else None
        print("  obj %d: body_start=%d, byte[143]=0x%02X (%r)" % (
            t, body_start, b143, bytes([b143]) if b143 else None))
        print("  context [140:150]: %r" % slice_[140:155])

