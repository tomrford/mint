#pragma once
#include <stdint.h>

#define NEO_ROW_COUNT 2u

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
} neo_config_t;
