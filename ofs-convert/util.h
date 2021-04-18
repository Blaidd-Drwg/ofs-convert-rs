#ifndef OFS_CONVERT_UTIL_H
#define OFS_CONVERT_UTIL_H

#include <stdint.h>

uint32_t log2(uint32_t value);

uint32_t min(uint32_t a, uint32_t b);

uint64_t from_lo_hi(uint32_t lo, uint32_t hi);

void set_lo_hi(uint32_t& lo, uint16_t& hi, uint64_t value);

void set_lo_hi(uint32_t& lo, uint32_t& hi, uint64_t value);

void set_lo_hi(uint16_t& lo, uint16_t& hi, uint32_t value);

void incr_lo_hi(uint16_t& lo, uint16_t& hi, uint32_t diff = 1);

void incr_lo_hi(uint32_t& lo, uint16_t& hi, uint64_t diff = 1);

void incr_lo_hi(uint32_t& lo, uint32_t& hi, uint64_t diff = 1);

void decr_lo_hi(uint16_t& lo, uint16_t& hi, uint32_t diff = 1);

template <class T>
uint32_t ceildiv(T a, T b) {
    return (a + b - 1) / b;
}

void bitmap_set_bit(uint8_t* bitmap, uint32_t bit_num);

void bitmap_set_bits(uint8_t* bitmap, uint32_t begin, uint32_t end);

#endif //OFS_CONVERT_UTIL_H
