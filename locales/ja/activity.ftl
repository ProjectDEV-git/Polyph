# アクティビティフィード。各行の横に実行者の名前と色が表示されます。
activity-card-created = { $column } に { $title } を作成しました
activity-card-moved = { $title } を { $from } から { $to } に移動しました
activity-card-updated = { $title } を編集しました
activity-card-removed = { $title } を削除しました
activity-merged = { $branch } を { $into } にマージしました
activity-merge-failed = { $branch } を { $into } にマージできませんでした
activity-changes-requested = レビューで { $branch } の変更が求められました
activity-agent-started = { $kind } エージェントを起動しました
activity-agent-stopped = エージェントを停止しました
activity-approval = { $approved ->
    [true] { $title } を承認しました
   *[other] { $title } を拒否しました
}
activity-overlap = { $agent } が { $path } を変更しましたが、このファイルは { $holder } がクレーム中です
