#pragma once
#include <stdint.h>

#define NEO_ROW_COUNT 2u
#define NEO_COUNT 1 + \
                  2

typedef enum {
    NEO_COLUMN_COUNT = 3
} neo_dimensions_t;

typedef struct {
    uint16_t channel;
    uint32_t limit;
} neo_inner_t;

typedef struct {
    uint16_t head;
    uint64_t wide;
    uint16_t tail;
} neo_cell_t;

/**
 * @mint block
 * @mint abi generic-le
 * @mint start-address 0
 */
typedef struct {
    uint16_t version;
    neo_inner_t inner;
    neo_cell_t cells[NEO_ROW_COUNT][NEO_COLUMN_COUNT];
    uint32_t matrix[2][2];
    uint16_t counts[NEO_COUNT * 2];
    uint16_t signed_mid[1 - 2 + 3];
    uint16_t unsigned_wrap[0u - 1u + 2u];
    uint16_t widened[((0xffffffffUL + 2LL) % 5) + 1];
    uint16_t abi_hex[((0x8000 * 2) % 5) + 1];
    float gain;
    double threshold;
} neo_config_t;
