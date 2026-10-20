"""Authored effects for the compatible complete entry-body grammars.

Every row is (instruction offset, destination role, destination displacement,
byte extent, phase). This is a static reviewed interface, never inferred from
an input binary. The matching complete ENTRY_GRAMMARS supplies the exact
register/branch/value provenance for these effects; a role alone is not an
address permission. Any new shape/effect requires source and full-body review.

The two shared Speed2 main/once shapes are reused by SizeS (and main by Z).
The Z spawn/push rows apply only to their explicitly bound first invocation;
later Dashboard invocations receive no boot identity. Post-initialize rows
partition the complete bodies but are not predecessors of the once claim.
"""

WRITES: dict[str, tuple[tuple[int, str, int, int, str], ...]] = {
    "speed2_0": (
        (0, "stack", -8, 8, "before"),
        (12, "boot", 40, 4, "before"),
        (16, "boot", 44, 1, "before"),
    ),
    "speed2_1": (
        (0, "stack", -8, 8, "after"),
        (16, "dashboard", 9744, 4, "after"),
        (30, "dashboard", 9748, 4, "after"),
        (38, "dashboard", 9780, 1, "after"),
    ),
    "speed2_2": (
        (0, "stack", -8, 8, "before"),
        (28, "boot", 24, 4, "before"),
        (44, "boot", 36, 4, "before"),
        (52, "boot", 32, 4, "before"),
        (56, "boot", 28, 4, "before"),
        (66, "executor", 0, 4, "before"),
        (76, "boot", 28, 4, "before"),
        (86, "executor", 0, 4, "before"),
        (96, "boot", 28, 4, "before"),
        (106, "executor", 0, 4, "before"),
        (118, "boot", 28, 4, "before"),
        (128, "executor", 0, 4, "before"),
        (208, "executor", 0, 4, "before"),
        (222, "boot", 28, 4, "before"),
        (224, "boot", 25, 1, "before"),
        (278, "stack", -16, 4, "diagnostic"),
    ),
    "speed2_3": (
        (0, "stack", -8, 8, "before"),
        (8, "stack", -16, 4, "before"),
        (14, "stack", -12, 4, "before"),
    ),
    "speed2_4": (
        (0, "stack", -20, 20, "before"),
        (4, "stack", -24, 4, "before"),
        (46, "dashboard", 24, 4, "after"),
        (62, "dashboard", 36, 4, "after"),
        (70, "dashboard", 32, 4, "after"),
        (76, "dashboard", 28, 4, "after"),
        (86, "executor", 0, 4, "after"),
        (96, "dashboard", 28, 4, "after"),
        (106, "executor", 0, 4, "after"),
        (116, "dashboard", 28, 4, "after"),
        (126, "executor", 0, 4, "after"),
        (136, "dashboard", 28, 4, "after"),
        (146, "executor", 0, 4, "after"),
        (216, "boot", 44, 1, "after"),
        (228, "boot", 36, 4, "after"),
        (232, "boot", 24, 1, "after"),
        (296, "stack", -32, 4, "after_diagnostic"),
    ),
    "speed2_5": ((0, "stack", -8, 8, "before"),),
    "speed2_6": (
        (0, "stack", -8, 8, "before"),
        (26, "Config", 14600, 1, "claim"),
    ),
    "sizes_0": (
        (0, "stack", -8, 8, "before"),
        (4, "boot", 40, 4, "before"),
        (8, "boot", 44, 1, "before"),
    ),
    "sizes_1": (
        (0, "stack", -8, 8, "after"),
        (10, "dashboard", 9744, 4, "after"),
        (20, "dashboard", 9748, 4, "after"),
        (24, "dashboard", 9780, 1, "after"),
    ),
    "sizes_2": (
        (0, "stack", -8, 8, "before"),
        (42, "boot", 24, 4, "before"),
        (70, "boot", 36, 4, "before"),
        (78, "boot", 32, 4, "before"),
        (84, "boot", 28, 4, "before"),
        (96, "executor", 0, 4, "before"),
        (128, "executor", 0, 4, "before"),
        (142, "boot", 28, 4, "before"),
        (144, "boot", 25, 1, "before"),
        (188, "stack", -16, 4, "diagnostic"),
    ),
    "sizes_3": (
        (0, "stack", -8, 8, "before"),
        (10, "stack", -12, 4, "before"),
        (14, "stack", -16, 4, "before"),
    ),
    "sizes_4": (
        (0, "stack", -20, 20, "before"),
        (4, "stack", -24, 4, "before"),
        (64, "dashboard", 24, 4, "after"),
        (92, "dashboard", 36, 4, "after"),
        (100, "dashboard", 32, 4, "after"),
        (104, "dashboard", 28, 4, "after"),
        (116, "executor", 0, 4, "after"),
        (144, "boot", 44, 1, "after"),
        (156, "boot", 36, 4, "after"),
        (160, "boot", 24, 1, "after"),
        (212, "stack", -32, 4, "after_diagnostic"),
    ),
    "sizez_0": (
        (0, "stack", -8, 8, "before"),
        (6, "boot", 40, 4, "before"),
        (8, "boot", 44, 1, "before"),
    ),
    "sizez_1": (
        (0, "stack", -8, 8, "after"),
        (8, "dashboard", 9744, 4, "after"),
        (14, "dashboard", 9780, 1, "after"),
        (20, "dashboard", 9748, 4, "after"),
    ),
    "sizez_2": (
        (0, "stack", -16, 16, "before"),
        (32, "boot", 24, 4, "before"),
        (86, "stack", -16, 4, "diagnostic"),
    ),
    "sizez_3": (
        (0, "stack", -16, 16, "before"),
        (8, "stack", -12, 4, "before"),
        (12, "stack", -16, 4, "before"),
    ),
    "sizez_4": (
        (0, "stack", -20, 20, "before"),
        (4, "stack", -24, 4, "before"),
        (60, "dashboard", 24, 4, "after"),
        (82, "dashboard", 36, 4, "after"),
        (102, "boot", 36, 4, "after"),
        (106, "boot", 44, 1, "after"),
        (112, "boot", 24, 1, "after"),
        (138, "stack", -32, 4, "after_diagnostic"),
    ),
    "sizez_6": (
        (0, "stack", -8, 8, "first_invocation_only"),
        (6, "boot", 28, 4, "first_invocation_only"),
        (18, "executor", 0, 4, "first_invocation_only"),
    ),
    "sizez_7": (
        (0, "stack", -16, 16, "before"),
        (12, "executor", 0, 4, "before"),
        (26, "boot", 28, 4, "before"),
        (28, "boot", 25, 1, "before"),
    ),
    "sizez_8": (
        (0, "stack", -8, 8, "first_invocation_only"),
        (4, "boot", 32, 4, "first_invocation_only"),
    ),
    "sizez_9": ((0, "boot", 36, 4, "before"),),
    "sizez_10": (
        (0, "stack", -8, 8, "before"),
        (20, "Config", 14600, 1, "claim"),
    ),
}
