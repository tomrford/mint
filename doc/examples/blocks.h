#ifndef MINT_CONFIG_DATA_H
#define MINT_CONFIG_DATA_H

#include <limits.h>
#include <stddef.h>
#include <stdint.h>

#define CONFIG_SCHEMA_FINGERPRINT UINT64_C(0x206A2310660BB1CF)

#define CONFIG_DEVICE_NAME_LEN 16u

#define CONFIG_FLAGS_ENABLE_DEBUG_SHIFT 0u
#define CONFIG_FLAGS_ENABLE_DEBUG_MASK UINT16_C(0x0001)

#define CONFIG_FLAGS_REGION_CODE_SHIFT 4u
#define CONFIG_FLAGS_REGION_CODE_MASK UINT16_C(0x00F0)

#define CONFIG_COEFFICIENTS_LEN 4u

#define CONFIG_MATRIX_ROWS 2u
#define CONFIG_MATRIX_COLS 2u

#define DATA_SCHEMA_FINGERPRINT UINT64_C(0xC1C13126EA0F1E6B)

#define DATA_CONFIG_SCHEMA_FINGERPRINT UINT64_C(0x206A2310660BB1CF)

#define DATA_MESSAGE_LEN 16u

#define DATA_IP_LEN 4u

typedef struct {
  uint64_t schema; /* fingerprint */
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
  uint64_t schema; /* fingerprint */
  uint64_t config_schema; /* fingerprint */
  uint64_t counter;
  uint8_t message[DATA_MESSAGE_LEN];
  uint8_t ip[DATA_IP_LEN];
  uint32_t checksum;
} data_t;

_Static_assert(offsetof(config_t, schema) * CHAR_BIT == 0u * 8u, "Mint ABI offset mismatch for config.schema");
_Static_assert(offsetof(config_t, device) * CHAR_BIT == 8u * 8u, "Mint ABI offset mismatch for config.device");
_Static_assert(offsetof(config_t, device.id) * CHAR_BIT == 8u * 8u, "Mint ABI offset mismatch for config.device.id");
_Static_assert(offsetof(config_t, device.name) * CHAR_BIT == 12u * 8u, "Mint ABI offset mismatch for config.device.name");
_Static_assert(offsetof(config_t, version) * CHAR_BIT == 28u * 8u, "Mint ABI offset mismatch for config.version");
_Static_assert(offsetof(config_t, gain_q8_8) * CHAR_BIT == 30u * 8u, "Mint ABI offset mismatch for config.gain_q8_8");
_Static_assert(offsetof(config_t, flags) * CHAR_BIT == 32u * 8u, "Mint ABI offset mismatch for config.flags");
_Static_assert(offsetof(config_t, coefficients) * CHAR_BIT == 36u * 8u, "Mint ABI offset mismatch for config.coefficients");
_Static_assert(offsetof(config_t, matrix) * CHAR_BIT == 52u * 8u, "Mint ABI offset mismatch for config.matrix");
_Static_assert(offsetof(config_t, checksum) * CHAR_BIT == 60u * 8u, "Mint ABI offset mismatch for config.checksum");
_Static_assert(sizeof(config_t) * CHAR_BIT == 64u * 8u, "Mint ABI size mismatch for config_t");

_Static_assert(offsetof(data_t, schema) * CHAR_BIT == 0u * 8u, "Mint ABI offset mismatch for data.schema");
_Static_assert(offsetof(data_t, config_schema) * CHAR_BIT == 8u * 8u, "Mint ABI offset mismatch for data.config_schema");
_Static_assert(offsetof(data_t, counter) * CHAR_BIT == 16u * 8u, "Mint ABI offset mismatch for data.counter");
_Static_assert(offsetof(data_t, message) * CHAR_BIT == 24u * 8u, "Mint ABI offset mismatch for data.message");
_Static_assert(offsetof(data_t, ip) * CHAR_BIT == 40u * 8u, "Mint ABI offset mismatch for data.ip");
_Static_assert(offsetof(data_t, checksum) * CHAR_BIT == 44u * 8u, "Mint ABI offset mismatch for data.checksum");
_Static_assert(sizeof(data_t) * CHAR_BIT == 48u * 8u, "Mint ABI size mismatch for data_t");

#endif /* MINT_CONFIG_DATA_H */
