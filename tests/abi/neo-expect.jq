def field($path): first(.fields[] | select(.path == $path));
def array($path): first(.arrays[] | select(.path == $path));

[
  "#pragma once",
  "#define NEO_ROOT_SIZE_BITS \(.root_size_octets * 8)",
  "#define NEO_ROOT_ALIGNMENT_BITS \(.alignment * 8)",
  "#define NEO_VERSION_OFFSET_BITS \((field("version").offset) * 8)",
  "#define NEO_INNER_OFFSET_BITS \((field("inner").offset) * 8)",
  "#define NEO_INNER_SIZE_BITS \((field("inner").size) * 8)",
  "#define NEO_INNER_ALIGNMENT_BITS \((field("inner").alignment) * 8)",
  "#define NEO_INNER_LIMIT_OFFSET_BITS \((field("inner.limit").offset) * 8)",
  "#define NEO_CELLS_OFFSET_BITS \((field("cells").offset) * 8)",
  "#define NEO_CELL_SIZE_BITS \((array("cells").stride) * 8)",
  "#define NEO_CELL_ALIGNMENT_BITS \((field("cells").alignment) * 8)",
  "#define NEO_CELL_WIDE_OFFSET_BITS \((field("cells[].wide").offset) * 8)",
  "#define NEO_MATRIX_OFFSET_BITS \((field("matrix").offset) * 8)",
  "#define NEO_MATRIX_ELEMENT_BITS \((array("matrix").stride) * 8)",
  "#define NEO_COUNTS_SIZE_BITS \((field("counts").size) * 8)",
  "#define NEO_GAIN_OFFSET_BITS \((field("gain").offset) * 8)",
  "#define NEO_THRESHOLD_OFFSET_BITS \((field("threshold").offset) * 8)"
] | .[]
