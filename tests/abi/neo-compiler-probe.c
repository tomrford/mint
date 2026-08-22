#include <limits.h>
#include <stddef.h>

#ifndef MINT_NEO_SCHEMA_HEADER
#define MINT_NEO_SCHEMA_HEADER "mint_neo.h"
#endif
#include MINT_NEO_SCHEMA_HEADER
#ifndef MINT_NEO_EXPECT_FROM_FLAGS
#include "mint_neo_expect.h"
#endif

#define OFFSET_BITS(type, member) (offsetof(type, member) * CHAR_BIT)
#define SIZE_BITS(type) (sizeof(type) * CHAR_BIT)
#define ALIGN_BITS(type) (_Alignof(type) * CHAR_BIT)

_Static_assert(SIZE_BITS(neo_config_t) == NEO_ROOT_SIZE_BITS, "root size");
_Static_assert(ALIGN_BITS(neo_config_t) == NEO_ROOT_ALIGNMENT_BITS, "root alignment");
_Static_assert(OFFSET_BITS(neo_config_t, version) == NEO_VERSION_OFFSET_BITS,
               "version offset");
_Static_assert(OFFSET_BITS(neo_config_t, inner) == NEO_INNER_OFFSET_BITS,
               "inner offset");
_Static_assert(SIZE_BITS(neo_inner_t) == NEO_INNER_SIZE_BITS, "inner size");
_Static_assert(ALIGN_BITS(neo_inner_t) == NEO_INNER_ALIGNMENT_BITS,
               "inner alignment");
_Static_assert(OFFSET_BITS(neo_config_t, inner) + OFFSET_BITS(neo_inner_t, limit) ==
                   NEO_INNER_LIMIT_OFFSET_BITS,
               "inner limit offset");
_Static_assert(OFFSET_BITS(neo_config_t, cells) == NEO_CELLS_OFFSET_BITS,
               "cells offset");
_Static_assert(SIZE_BITS(neo_cell_t) == NEO_CELL_SIZE_BITS, "cell stride");
_Static_assert(ALIGN_BITS(neo_cell_t) == NEO_CELL_ALIGNMENT_BITS, "cell alignment");
_Static_assert(OFFSET_BITS(neo_config_t, cells) + OFFSET_BITS(neo_cell_t, wide) ==
                   NEO_CELL_WIDE_OFFSET_BITS,
               "cell wide offset");
_Static_assert(sizeof(((neo_config_t *)0)->cells) == 6 * sizeof(neo_cell_t),
               "record array dimensions");
_Static_assert(OFFSET_BITS(neo_config_t, matrix) == NEO_MATRIX_OFFSET_BITS,
               "matrix offset");
_Static_assert(SIZE_BITS(uint32_t) == NEO_MATRIX_ELEMENT_BITS, "matrix stride");
_Static_assert(sizeof(((neo_config_t *)0)->matrix) == 4 * sizeof(uint32_t),
               "scalar array dimensions");

int mint_neo_abi_probe(neo_config_t *config) {
  return (int)(config->version + config->inner.limit + config->cells[1][2].wide +
               config->matrix[1][1]);
}
