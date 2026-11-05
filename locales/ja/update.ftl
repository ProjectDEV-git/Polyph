# polyph update: replaces an install made with the install script by a newer release.
help-update = polyph を最新リリースに更新します。
update-unsupported = このシステム向けのリリースビルドはありません。ソースから polyph をビルドしてください。
update-not-archive = この polyph（{ $path }）はインストールスクリプトで入れたものではないため、自分では更新できません。インストールしたときと同じ方法で更新してください。デスクトップアプリと VS Code 拡張機能には専用の polyph が入っています。
update-no-release = github.com/{ $repo } にリリースが見つかりません。
update-no-build = このリリースにはこのシステム（{ $target }）向けのビルドがありません。
update-unreachable = 更新を確認できませんでした: { $error }
update-available = polyph { $version } が利用できます（現在は { $current }）。`polyph update` でインストールします。
update-current = polyph { $version } が最新リリースです。
update-downloading = { $file } をダウンロード中
update-checksum = ダウンロードしたファイルがチェックサムと一致しません（{ $file }）。何も変更していません。
update-failed = { $dir } のファイルを更新できませんでした: { $error }
update-done = polyph を { $from } → { $to } に更新しました。
update-daemon-restarted = デーモンを新しいバージョンで再起動しました。
update-daemon-restart-failed = デーモンは停止しています。次に polyph コマンドを使うと起動します。
update-daemon-busy = エージェントが { $count } 件作業中のため、デーモンは古いバージョンのまま動いています。終わったら再起動してください: polyph daemon stop のあと polyph daemon start。

# polyph uninstall
help-uninstall = このコンピューターから polyph を削除します。
uninstall-not-archive = この polyph（{ $path }）はインストールスクリプトで入れたものではありません。インストールしたときと同じ方法で削除してください。
uninstall-needs-yes = polyph の削除には確認が必要です。ターミナルで実行するか、--yes を付けてください。
uninstall-what = { $dir } から polyph、polyph-relay、ブラウザークライアントを削除し、デーモンと動作中のエージェントを停止します。
uninstall-purge-what = { $dir } にあるこのデバイスの Polyph データ（デバイスの ID、ルーム、設定）も削除します。このデバイスにしかないルームは失われます。
uninstall-confirm = 続けますか？ [y/N]
uninstall-cancelled = 何も削除していません。
uninstall-failed = { $dir } のファイルを削除できませんでした: { $error }
uninstall-done = polyph をアンインストールしました。
uninstall-kept-data = Polyph のデータは再インストールに備えて { $dir } に残っています。これも削除するには、そのフォルダーを削除してください。
