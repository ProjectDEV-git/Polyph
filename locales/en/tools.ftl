tool-read = { $count ->
    [one] Read 1 file ({ $files })
   *[other] Read { $count } files ({ $files })
}
tool-search = Searched for { $query } ({ $matches ->
    [one] 1 match
   *[other] { $matches } matches
})
tool-search-plain = Searched for { $query }
tool-search-many = Ran { $count } searches
tool-edit = Edited { $file } (+{ $added } −{ $removed })
tool-delete = Deleted { $files }
tool-move = Moved { $files }
tool-run = Ran { $command }
tool-run-tests = Ran { $command }: { $passed } passed, { $failed } failed
tool-run-failed = Ran { $command } (exit code { $code })
tool-think = Thought it through
tool-fetch = { $count ->
    [one] Fetched { $target }
   *[other] Fetched { $count } pages
}
tool-mcp = Used { $tool }
tool-other = { $title }
