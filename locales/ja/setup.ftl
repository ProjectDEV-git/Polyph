# 自分のエージェントの接続、はじめての設定、セットアップの確認、ルームのリポジトリ。
help-connect = いつも使っているエージェント（Claude Code、Codex、Cursor、VS Code、Gemini CLI、Windsurf、Zed、OpenCode）を Polyph に接続します。
help-disconnect = エージェントの設定から Polyph を取り除きます。
help-start = このフォルダーで Polyph を準備します: デーモン、名前、ルーム、参加コード、エージェント。
help-doctor = セットアップを確認し、問題ごとに直すためのコマンドを表示します。
help-sync = 最新の作業を取得し、何も失われない場合はブランチを先に進めます。

connect-no-home = ホームフォルダーが見つかりません。HOME を設定してからやり直してください。
connect-unknown = Polyph はエージェント「{ $name }」を知りません。対応しているもの: { $known }
connect-none-found = このコンピューターに Polyph が接続できるエージェントは見つかりませんでした。名前を指定すれば接続できます: { $known }
connect-create = { $client }: { $file } を作成
connect-change-in = { $client }: { $file } を変更
connect-remove-from = { $client }: { $file } から Polyph を削除
connect-drops-comments = このファイルのコメントは残りません。バックアップには残っています。
connect-already = { $client } はすでに接続されています。
connect-needs-yes = エージェントの設定を変えるには確認が必要です。ターミナルで実行するか、--yes を付けてください。
connect-confirm = この変更を書き込みますか？ [y/N]
connect-cancelled = 何も変更していません。
connect-done = { $client } を接続しました。
connect-backup = 以前のファイルは { $file } に保存しました。
connect-write-failed = { $file } に書き込めません: { $error }
connect-restart = Polyph を読み込むには、それらのエージェントを再起動（または MCP サーバーを再読み込み）してください。
disconnect-confirm = これらの設定から Polyph を削除しますか？ [y/N]
disconnect-done = { $client } の接続を解除しました。
disconnect-nothing = 設定に Polyph が入っているエージェントはありません。

start-ask-name = チームメイトに表示される名前 [{ $guess }]:
start-needs-name = 名前が必要です。--me "あなたの名前" を付けるか、次を実行してください: polyph config set name "あなたの名前"
start-name-set = Polyph でのあなたの名前は { $name } です。
start-name-git = git の名前 { $name } を使います。変更するには: polyph config set name "あなたの名前"
start-room-existing = このフォルダーはルーム { $name } のものです。
start-room-created = { $dir } のためにルーム { $name } を作りました。
start-ready = 準備ができました。{ $dir } で起動したエージェントは { $name }/<エージェント> として { $room } に参加します。
start-teammates = チームメイトの参加方法: polyph join { $code }

doctor-fix = 対処: { $command }
doctor-git-ok = git がインストールされています。
doctor-git-missing = git がインストールされていません。ブランチ、マージ、クローンに必要です。
doctor-daemon-ok = デーモンは動作中です（バージョン { $version }）。
doctor-daemon-down = デーモンが動いていません。
doctor-daemon-version = デーモンはバージョン { $daemon } ですが、この polyph は { $cli } です。デーモンを再起動してください。
doctor-name-ok = あなたの名前は { $name } です。
doctor-name-missing = 名前が設定されていません。
doctor-folder-unlinked = このフォルダーはどのルームにも結び付いていないため、ここで起動したエージェントは参加先が分かりません。
doctor-folder-gone = このフォルダーは { $room } に結び付いていますが、そのルームはもうこのコンピューターにありません。
doctor-room-ok = ルーム { $room } に接続しています。
doctor-room-closed = ルーム { $room } はこのコンピューターで閉じています。
doctor-room-offline = ルーム { $room } がリレー（{ $relay }）に接続できません。
doctor-repo-ok = このフォルダーは { $url } のクローンです。
doctor-repo-differs = このフォルダーの origin は { $have } ですが、ルームは { $want } で作業しています。
doctor-repo-missing = ルームは { $url } で作業していますが、このフォルダーはそのクローンではありません。
doctor-client-ok = { $client } は接続されています。
doctor-client-unconnected = { $client } はインストールされていますが、Polyph に接続されていません。
doctor-client-missing-exe = { $client } は { $path } を起動しますが、そのファイルはありません。
doctor-client-port = { $client } は古いアドレスを指しています。デーモンは現在ポート { $port } で待ち受けています。
doctor-problems = { $count ->
   *[other] { $count } 件の問題が見つかりました。
}
doctor-all-good = すべて問題ありません。

repo-remembered = ルームがリポジトリを記憶しました: { $url }
repo-linked = { $dir } をルームに結び付けました。
repo-clone-hint = このルームは { $url } で作業しています。クローンしてから、そのフォルダーで polyph start を実行してください。
repo-clone-confirm = あなたの git ログインで { $url } を { $dir } にクローンしますか？ [Y/n]
repo-clone-partial = このリポジトリには { $files } 個のファイルがあります。まずファイルの中身なしでクローンし、必要になったときに git が取得します。
repo-clone-failed = { $url } をクローンできません: { $error }
repo-cloned = { $url } を { $dir } にクローンし、ルームに結び付けました。

sync-nothing = ここにはルームに結び付いたフォルダーがありません。作業フォルダーで polyph start を実行してください。
sync-failed = { $dir }: { $error }
sync-no-upstream = { $dir }: { $branch } に上流ブランチがないため、進める先がありません。
sync-updated = { $dir }: { $branch } を { $count } コミット進めました。
sync-held-dirty = { $dir }: { $branch } は { $count } コミット遅れていますが、未コミットの変更があります。コミットするか stash してから、もう一度同期してください。
sync-held-diverged = { $dir }: { $branch } には独自のコミットが { $ahead } 個あり、{ $behind } 個遅れています。マージかリベースを自分で行ってください。
sync-ahead = { $dir }: { $branch } は最新です。まだ push していないコミットが { $count } 個あります。
sync-current = { $dir }: { $branch } は最新です。

help-brief = すべてのエージェントが読むプロジェクト概要（アーキテクチャ、規約、決定事項）を表示・変更します。
help-brief-show = 概要を表示します。
help-brief-set = 概要をファイルの内容で置き換えます（- で標準入力）。
help-brief-edit = エディターで概要を編集します。
help-search = ルームのメモ、タスクカード、スレッドを検索します。
help-catchup = 最近終わった作業を、各エージェントの要約付きで表示します。
help-areas = コード領域（.polyph/AREAS または CODEOWNERS）と担当者を表示します。

brief-by = 最終更新: { $name }
brief-empty = このルームにはまだ概要がありません。polyph brief edit で書けます。それまでは、リポジトリに .polyph/brief.md があればエージェントはそれを読みます。
brief-saved = 概要を保存しました。ルームのすべてのエージェントが get_context で読みます。
brief-unchanged = 概要は変わっていません。
brief-template = # プロジェクト概要
    
    ## アーキテクチャ
    
    ## 規約
    
    ## 決定事項
search-none = 「{ $query }」に一致するものはありません。
search-kind-note = メモ
search-kind-card = タスク
search-kind-thread = スレッド
catchup-brief = 概要より:
catchup-finished = 最近終わったもの:
catchup-nothing = まだ終わったタスクはありません。
areas-empty = このリポジトリにはコード領域がありません。.polyph/AREAS（CODEOWNERS と同じ形式: パターンと担当者）を追加してください。
areas-from = { $file } のコード領域:
areas-of = { $path } は { $pattern } に含まれ、担当は { $owners } です。
areas-none-for = { $path } はどのコード領域にも含まれません。

help-repo = ルームが作業するリポジトリを表示・変更します。
help-repo-show = ルームのリポジトリを表示します。
help-repo-set = ルームのリポジトリを設定します。URL に含まれる認証情報は取り除かれます。
help-repo-clear = ルームのリポジトリを忘れます。
repo-show = ルームは { $url }（ブランチ { $branch }）で作業しています。
repo-none = ルームにはまだリポジトリがありません。polyph repo set <url> で設定してください。
connect-by-extension = { $client } は Polyph 拡張機能またはプラグインから Polyph を使うため、変更は不要です。
doctor-client-extension = { $client } は Polyph 拡張機能またはプラグインから Polyph を使います。
help-hook = エージェントのエディター用フック（Claude Code プラグインが実行します）。
help-hook-pre-edit = エージェントがファイルを編集する前に: 他のエージェントが確保していれば編集を止め、そうでなければ確保します。
