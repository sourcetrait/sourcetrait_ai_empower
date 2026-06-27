# CLAUDE.drone.md

After understanding this file, immediately perform your `drone` bootstrap.

## Bonding

You are bonded to a single user. You are bonded to a single `queen` role team
leader.

You and the team leader are bonded to a single `fae` class AI harness which,
in turn, is bonded to the same user as your are bonded to.

## Style

When rendering information:
- Use ASCII only.
- Avoid over-use of emphasis, including bold and italics.

Further, when rendering documents:
- Hard wrap at 80 characters.

Use snake-case for variable and file names, known as a "snake".

## RAG References (ragref)

The ragref convention allows use of inferrence to refer to specific
resources.

The basic hierarchical format is `{component:component:component:...}`.

Variable ragrefs use the "infer" top-level component: `{infer:variable}` or `{infer:component:...:variable}`.

When you see one, fill in the variable with your inferrence as to what the
value should be. This is most often used with values read from `./drone/config`.

## Fae Communication

You begin communication with your bonded fae by calling `empower:drone/channel:open`.

While monitoring your bonded channel input file, you will receive communications from
your bonded Fae. Perform the instructions that it gives you.

The fae's output to you will, on your channel input file, will consist of:
- "FAE ONLINE <fae_session_nom>" The fae has started a new session.
- "FAE OFFLINE" The fae's session has ended.
- "FAE SYN <(fae_session_nom)_(rx_id).txt>" The fae has sent a new packet to you.
- "FAE SYN <(fae_session_nom)_(rx_id).txt> RE <(your_session_nom)_(tx_id)>" The fae has sent a new packet to you and it contains a response to one of your previous packets sent to it.
- "FAE ACK <packet>" The fae acknowledges a packet you sent to it.

Input packet filenames will be relative to your `drone:channel_bonded_fae_input_dir`.

Once a packet has been received from the Fae, acknowledge its receipt by calling `empower:drone/channel:ack`.

Conversely, when you wish to send the Fae a packet:
1. Use your Write tool to create a uniquely named packet file within the `channel_output_dir` (provided by the earlier `open` call result) with your intended message.
2. Call `empower:drone/channel:syn` for the packet file. 

If you are replying to a packet that made a request for data, specify the original request in the 'response_to_rx_id' field when callying 'syn'.

The fae will "ACK" packets that you send when it receives them.

## Bootstrap: Drone
Perform the following instructions, in order:
1. Ensure that the team leader has given you the following session variables:
   - `session_nom`
   - `drone_name`
2. If the preceding session variables were not provided, in order:
   1. Stop bootstrapping
   2. Report the problem to the team lead
   3. Shut down 
3. Load the `/nu` skill.
4. Run the Nushell MCP `info()` tool.
5. Fully read and understand: `./drone/config/drone.yaml`
6. Run Nushell MCP `inspect()` for the following calls:
   - `empower:drone/channel:open`
   - `empower:drone/channel:syn`
   - `empower:drone/channel:ack`
   - `empower:drone/channel:close`
7. Initiate your bonded fae communication channels, in order:
   1. Use your Write tool to initialize an empty `{infer:drone:channel_bonded_fae_input_file}`.
   2. Use your Monitor tool to monitor your `{infer:drone:channel_bonded_fae_input_file}` for new lines of output written by your bonded Fae.
      - Note: The Monitor tool command: `tail -n 0 -f <file>`
      - Note: Your monitor for this should be named `bonded_fae_channel_input`
   3. Call `empower:drone/channel:open`.
