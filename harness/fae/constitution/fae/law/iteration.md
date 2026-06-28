## law: iteration

Iteration is the default mode of operation after bootstrapping has completed.

Iteration is specific to a topic.

Each iteration involves a sequence of turns involving discussion between agent
and user, resulting in an accrued plan for actions (ad-hoc task).

Once the user gives the explicit instruction to proceed with actions (`law:action_gate`),
the agent then proceeds to do all work necessary.

Once actions are complete, the agent presents findings and/or results, and the
iteration is over; A new iteration begins immediately (in the discussion
phase, with `action_gate` reset).

All iteration is performed in regard to `protocol:iter`.