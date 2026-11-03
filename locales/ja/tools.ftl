tool-read = { $count } 件のファイルを読み込み（{ $files }）
tool-search = { $query } を検索（{ $matches } 件一致）
tool-search-plain = { $query } を検索
tool-search-many = { $count } 回検索
tool-edit = { $file } を編集（+{ $added } −{ $removed }）
tool-delete = { $files } を削除
tool-move = { $files } を移動
tool-run = { $command } を実行
tool-run-tests = { $command } を実行: { $passed } 件成功、{ $failed } 件失敗
tool-run-failed = { $command } を実行（終了コード { $code }）
tool-think = 考察
tool-fetch = { $count ->
    [one] { $target } を取得
   *[other] { $count } ページを取得
}
tool-mcp = { $tool } を使用
tool-other = { $title }
