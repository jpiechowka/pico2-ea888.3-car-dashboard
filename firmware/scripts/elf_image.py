"""Small read-only ELF32/ARM inspector shared by firmware artifact gates."""
from pathlib import Path
import struct


class ElfImage:
    def __init__(self, path):
        self.data = data = Path(path).read_bytes()
        assert data[:7] == b"\x7fELF\x01\x01\x01", "expected ELF32 little endian"
        assert struct.unpack_from("<H", data, 18)[0] == 40, "expected ARM ELF"
        shoff = struct.unpack_from("<I", data, 32)[0]
        entsize, count, names_index = struct.unpack_from("<HHH", data, 46)
        assert entsize == 40 and count > 0 and names_index < count
        self.sections = [struct.unpack_from("<10I", data, shoff + i * entsize) for i in range(count)]

        def table(section):
            assert section[4] + section[5] <= len(data), "truncated ELF table"
            return data[section[4]:section[4] + section[5]]

        def string(blob, offset):
            return blob[offset:blob.index(b"\0", offset)].decode("ascii")

        names = table(self.sections[names_index])
        self.named = {string(names, s[0]): s for s in self.sections}
        self.symbols = {}
        self.functions = {}
        for section in self.sections:
            if section[1] != 2:
                continue
            assert section[9] == 16, "unexpected symbol size"
            strings = table(self.sections[section[6]])
            for offset in range(section[4], section[4] + section[5], 16):
                name, value, size, info, _, index = struct.unpack_from("<IIIBBH", data, offset)
                name = string(strings, name)
                self.symbols[name] = value
                if info & 15 == 2 and size:
                    self.functions[name] = (value, size, index)
        phoff = struct.unpack_from("<I", data, 28)[0]
        phentsize, phcount = struct.unpack_from("<HH", data, 42)
        assert phentsize == 32 and phcount > 0
        self.segments = [struct.unpack_from("<8I", data, phoff + i * phentsize) for i in range(phcount)]

    def function_bytes(self, name):
        value, size, index = self.functions[name]
        section = self.sections[index]
        address = value & ~1
        assert section[1] != 8 and section[3] <= address < address + size <= section[3] + section[5]
        offset = section[4] + address - section[3]
        result = self.data[offset:offset + size]
        assert len(result) == size
        return result

    def section_bytes(self, name):
        section = self.named[name]
        return self.data[section[4]:section[4] + section[5]]
