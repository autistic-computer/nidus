# Audit

Counts every literal length in penna's components and blocks, Rust and CSS together, against the proposed scales.

91% of spacing values and 95% of font sizes already land on the scales. Line heights are the weak spot (70%), and the numbers miss the biggest case: native text with no style at all, which takes ala's 1.618 line height. Only 6% of spacing values are written with the `Space` token.

Measured on main at faf29da. Hover a bar for its Rust and CSS counts; each chart has a table.
