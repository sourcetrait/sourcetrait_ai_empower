## protocol: iter_journal

Journals exist in `./iter/<topic>/journal.md` in order to record meta
information that is *not* captured anywhere else in the repository.

They exist primarily record the meta information surrounding a change that
happened to some other form(s) of information store (memories, iter, documents,
etc).

 Examples of historical meta information:
- When a change occured
- Who requested the change (eg, "`the_user` said...")
- What was the previous information 

Inversely, the journaling protocol dictates that information intended to
reflect current truth should never include historical meta information in its
content, opting to capture it in journaling instead. This includes:
- Memories
- Iter 
- Understanding within Iter

It is largely correct to apply it to most other documents as well, except for
obvious things like `./state` and `historical` directories.