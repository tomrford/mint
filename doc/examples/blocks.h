#ifndef MINT_CONFIG_DATA_H
#define MINT_CONFIG_DATA_H

#include <stdint.h>

#define CONFIG_DEVICE_NAME_LEN 16u

#define CONFIG_FLAGS_ENABLE_DEBUG_SHIFT 0u
#define CONFIG_FLAGS_ENABLE_DEBUG_MASK UINT16_C(0x0001)

#define CONFIG_FLAGS_REGION_CODE_SHIFT 4u
#define CONFIG_FLAGS_REGION_CODE_MASK UINT16_C(0x00F0)

#define CONFIG_COEFFICIENTS_LEN 4u

#define CONFIG_MATRIX_ROWS 2u
#define CONFIG_MATRIX_COLS 2u

#define DATA_MESSAGE_LEN 16u

#define DATA_IP_LEN 4u

typedef struct {
  struct {
    uint32_t id;
    uint8_t name[CONFIG_DEVICE_NAME_LEN];
  } device;
  uint16_t version;
  uint16_t gain_q8_8; /* uq8.8 */
  uint16_t flags; /* bitmap storage */
  float coefficients[CONFIG_COEFFICIENTS_LEN];
  int16_t matrix[CONFIG_MATRIX_ROWS][CONFIG_MATRIX_COLS];
  uint32_t checksum;
} config_t;

typedef struct {
  uint64_t counter;
  uint8_t message[DATA_MESSAGE_LEN];
  uint8_t ip[DATA_IP_LEN];
  uint32_t checksum;
} data_t;

#endif /* MINT_CONFIG_DATA_H */
