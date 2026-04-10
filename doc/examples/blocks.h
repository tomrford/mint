#ifndef EXAMPLES_BLOCKS_H
#define EXAMPLES_BLOCKS_H

#include <stdint.h>

/*
 * C structs corresponding to doc/examples/block.toml.
 * Dotted paths map to nested struct members.
 */

typedef struct {
  struct {
    uint32_t id;
    uint8_t name[16];
  } device;
  uint16_t version;
  uint16_t flags; /* bitmap */
  float coefficients[4];
  int16_t matrix[2][2];
  uint32_t checksum;
} config_t;

typedef struct {
  uint64_t counter;
  uint8_t message[16];
  uint8_t ip[4];
  uint32_t checksum;
} data_t;

#endif /* EXAMPLES_BLOCKS_H */
