# Append an ACK line to <channel_dir>/input.txt; errors if absent.
#
# Writes one line "<CLASS> ACK <packet_filename>" to the channel control file
# <channel_dir>/input.txt (true-appended), acknowledging receipt of a packet
# the peer announced. author_class is upper-cased to form the prefix (fae ->
# FAE, colony -> COLONY). packet_filename is the name of the received packet
# being acknowledged. Errors if the control file does not exist. Void return.
export def main [args: record<author_class: string, channel_dir: directory, packet_filename: string>]: nothing -> nothing {
    let channel_file = ($args.channel_dir | path join "input.txt")
    if not ($channel_file | path exists) {
        error make {msg: $"channel file does not exist: ($channel_file)"}
    }
    $"($args.author_class | str upcase) ACK ($args.packet_filename)(char nl)" | save --append $channel_file
}
