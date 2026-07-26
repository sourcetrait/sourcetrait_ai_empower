# mode.rs

## enum Mode
A named type rather than a bool because it stands for exactly ONE asymmetry
between the two eval engines - the stateful base additionally layers the
`plugin add/rm/list/use/stop` admin family - and naming it keeps that asymmetry
legible at every call site instead of leaving a caller to remember which way
`true` pointed.
