## protocol: iter_understood

Each Iter topic (`./iter/<topic>`) should have a `understood` sub-directory
that contains the entire working knowledge and understanding for that topic,
documented by Claude.

The markdown files represent the knowledge as a whole, categorized
broadly between 8-16 categories.

Beyond the categories, there must always be a `00_References.md` file that
acts purely as a directory-wide `ragref` for the entire understanding of a
topic when cross-referencing outside of that directory. Each category file must 
reference back to that ragref rather than providing its own.

Example of a directory-wide RAG reference:
```md
# References
## ref
- `mem:info-development` `{development}`
<EOF>
```

The working knowledge *must* be understood as a whole when working with its
topic.

The working understanding must be updated whenever new information concerning that
topic is identified, excluding task-meta details (follows, debt, bootstrapping,
etc.). Ensure that all design choices by The User are re-interpreted there.

Topic-specific information should be captured in 'understood' instead of memory,
as memory is intended for broad harness-wide information.

Working understanding should always reflect current truth. Historical meta
information should be reflected elsewhere, typically in journals.