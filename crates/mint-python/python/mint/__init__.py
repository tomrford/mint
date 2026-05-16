from ._native import BlockStat, BuildBlock, BuildResult, BuildStats, DataRange, Layout, build

# Planned hexy interop after hexy-python is cleaned up and released.
#
# def _range_to_hexy(self):
#     import hexy
#
#     return hexy.HexFile.from_segments([hexy.Segment(self.start_address, self.data)])
#
#
# def _result_to_hexy(self):
#     import hexy
#
#     return hexy.HexFile.from_segments(
#         [hexy.Segment(r.start_address, r.data) for r in self.ranges]
#     )
#
#
# DataRange.to_hexy = _range_to_hexy
# BuildResult.to_hexy = _result_to_hexy

__all__ = [
    "BlockStat",
    "BuildBlock",
    "BuildResult",
    "BuildStats",
    "DataRange",
    "Layout",
    "build",
]
