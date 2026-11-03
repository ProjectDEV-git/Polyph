tool-read = 读取了 { $count } 个文件（{ $files }）
tool-search = 搜索 { $query }（{ $matches } 处匹配）
tool-search-plain = 搜索 { $query }
tool-search-many = 进行了 { $count } 次搜索
tool-edit = 编辑了 { $file }（+{ $added } −{ $removed }）
tool-delete = 删除了 { $files }
tool-move = 移动了 { $files }
tool-run = 运行了 { $command }
tool-run-tests = 运行了 { $command }：{ $passed } 个通过，{ $failed } 个失败
tool-run-failed = 运行了 { $command }（退出码 { $code }）
tool-think = 进行了思考
tool-fetch = { $count ->
    [one] 获取了 { $target }
   *[other] 获取了 { $count } 个页面
}
tool-mcp = 使用了 { $tool }
tool-other = { $title }
