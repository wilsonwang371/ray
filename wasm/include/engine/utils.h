#include <iostream>
#pragma once

using namespace std;

inline void print_hex(uint8_t *data, size_t offset, size_t length) {
  // print column value
  cerr << "offset    00 01 02 03 04 05 06 07 08 09 0a 0b 0c 0d 0e 0f" << endl;
  size_t off_begin = offset >> 4 << 4;
  size_t off_end = ((offset + length) >> 4 << 4) + 0xf;
  for (size_t i = off_begin; i < off_end; i++) {
    if (i % 16 == 0) {
      cerr << hex << setfill('0') << setw(8) << i << ": ";
    }
    if (i >= offset && i < offset + length) {
      cerr << hex << setw(2) << (int)data[i] << " ";
    } else {
      cerr << "   ";
    }
    if (i % 16 == 15) {
      cerr << endl;
    }
  }
  if (off_end % 16 != 0) {
    cerr << endl;
  }
}
