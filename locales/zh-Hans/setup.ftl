# 连接你自己的智能体、开始使用、检查设置、房间的仓库。
help-connect = 将你正在使用的智能体（Claude Code、Codex、Cursor、VS Code、Gemini CLI、Windsurf、Zed、OpenCode）连接到 Polyph。
help-disconnect = 从智能体的设置中移除 Polyph。
help-start = 在此文件夹中设置 Polyph：守护进程、你的名字、房间、加入码和你的智能体。
help-doctor = 检查设置，并为每个问题给出修复命令。
help-sync = 获取最新工作，并在不会丢失任何内容时将你的分支向前推进。

connect-no-home = 找不到你的主文件夹；请设置 HOME 后重试。
connect-unknown = Polyph 不认识智能体“{ $name }”。已知的有：{ $known }。
connect-none-found = 在这台电脑上没有找到 Polyph 可以连接的智能体。指定名称仍可连接：{ $known }。
connect-create = { $client }：创建 { $file }
connect-change-in = { $client }：修改 { $file }
connect-remove-from = { $client }：从 { $file } 中移除 Polyph
connect-drops-comments = 此文件中的注释不会保留；备份中仍有。
connect-already = { $client } 已连接。
connect-needs-yes = 修改智能体的设置需要确认；请在终端中运行，或加上 --yes。
connect-confirm = 写入这些更改？[y/N]
connect-cancelled = 未做任何更改。
connect-done = 已连接 { $client }。
connect-backup = 原文件已保存为 { $file }。
connect-write-failed = 无法写入 { $file }：{ $error }
connect-restart = 重启这些智能体（或重新加载它们的 MCP 服务器）以加载 Polyph。
disconnect-confirm = 从这些设置中移除 Polyph？[y/N]
disconnect-done = 已断开 { $client }。
disconnect-nothing = 你的智能体设置中都没有 Polyph。

start-ask-name = 队友看到的你的名字 [{ $guess }]：
start-needs-name = Polyph 需要先知道你的名字：polyph config set name "你的名字"
start-name-set = 你在 Polyph 中的名字是 { $name }。
start-room-existing = 此文件夹属于房间 { $name }。
start-room-created = 已为 { $dir } 创建房间 { $name }。
start-ready = 准备就绪。你在 { $dir } 中启动的智能体会以 { $name }/<智能体> 的身份加入 { $room }。
start-teammates = 队友用此命令加入：polyph join { $code }

doctor-fix = 修复：{ $command }
doctor-git-ok = 已安装 git。
doctor-git-missing = 未安装 git；Polyph 需要它来处理分支、合并和克隆。
doctor-daemon-ok = 守护进程正在运行（版本 { $version }）。
doctor-daemon-down = 守护进程未运行。
doctor-daemon-version = 守护进程是 { $daemon } 版，而此 polyph 是 { $cli } 版；请重启守护进程。
doctor-name-ok = 你的名字是 { $name }。
doctor-name-missing = 你还没有设置名字。
doctor-folder-unlinked = 此文件夹未关联到任何房间，所以在这里启动的智能体不知道该加入哪个房间。
doctor-folder-gone = 此文件夹关联到 { $room }，但这台电脑上已没有该房间。
doctor-room-ok = 已连接到房间 { $room }。
doctor-room-closed = 房间 { $room } 在这台电脑上已关闭。
doctor-room-offline = 房间 { $room } 无法连接到中继（{ $relay }）。
doctor-repo-ok = 此文件夹是 { $url } 的克隆。
doctor-repo-differs = 此文件夹的 origin 是 { $have }，但房间使用的是 { $want }。
doctor-repo-missing = 房间使用的是 { $url }，但此文件夹不是它的克隆。
doctor-client-ok = { $client } 已连接。
doctor-client-unconnected = 已安装 { $client }，但尚未连接到 Polyph。
doctor-client-missing-exe = { $client } 启动的是 { $path }，但该文件不存在。
doctor-client-port = { $client } 指向旧的地址；守护进程现在监听端口 { $port }。
doctor-problems = { $count ->
   *[other] 发现 { $count } 个问题。
}
doctor-all-good = 一切正常。

repo-remembered = 房间已记住它的仓库：{ $url }
repo-linked = 已将 { $dir } 关联到房间。
repo-clone-hint = 此房间使用 { $url }。克隆它，然后在克隆的文件夹中运行 polyph start。
repo-clone-confirm = 使用你的 git 登录将 { $url } 克隆到 { $dir }？[Y/n]
repo-clone-partial = 此仓库有 { $files } 个文件：先克隆不含文件内容的版本，git 会在你需要时下载。
repo-clone-failed = 无法克隆 { $url }：{ $error }
repo-cloned = 已将 { $url } 克隆到 { $dir } 并关联到房间。

sync-nothing = 这里没有关联到房间的文件夹；请在你的代码目录中运行 polyph start。
sync-failed = { $dir }：{ $error }
sync-no-upstream = { $dir }：{ $branch } 没有上游分支，所以无处可推进。
sync-updated = { $dir }：已将 { $branch } 向前推进 { $count } 个提交。
sync-held-dirty = { $dir }：{ $branch } 落后 { $count } 个提交，但你有未提交的更改；请先提交或 stash，然后再同步。
sync-held-diverged = { $dir }：{ $branch } 有 { $ahead } 个自己的提交，并落后 { $behind } 个；请自行合并或变基。
sync-ahead = { $dir }：{ $branch } 已是最新，有 { $count } 个提交尚未推送。
sync-current = { $dir }：{ $branch } 已是最新。

help-brief = 查看或修改每个智能体都会读的项目简介：架构、约定、决策。
help-brief-show = 显示简介。
help-brief-set = 用文件内容替换简介（- 表示从标准输入读取）。
help-brief-edit = 在编辑器中编辑简介。
help-search = 搜索房间的笔记、任务卡片和讨论串。
help-catchup = 最近完成的工作，以及每个智能体留下的总结。
help-areas = 显示代码区域（.polyph/AREAS 或 CODEOWNERS）及其负责人。

brief-by = 最后由 { $name } 编辑。
brief-empty = 此房间还没有简介。用 polyph brief edit 编写；在此之前，如果仓库中有 .polyph/brief.md，智能体会读取它。
brief-saved = 简介已保存；房间里的每个智能体都会通过 get_context 读取它。
brief-unchanged = 简介没有变化。
brief-template = # 项目简介
    
    ## 架构
    
    ## 约定
    
    ## 决策
search-none = 没有与“{ $query }”匹配的内容。
search-kind-note = 笔记
search-kind-card = 任务
search-kind-thread = 讨论串
catchup-brief = 简介摘录：
catchup-finished = 最近完成：
catchup-nothing = 还没有完成的任务。
areas-empty = 此仓库没有代码区域。添加 .polyph/AREAS（格式同 CODEOWNERS）：先写路径模式，再写负责人。
areas-from = 来自 { $file } 的代码区域：
areas-of = { $path } 属于 { $pattern }，由 { $owners } 负责。
areas-none-for = { $path } 不属于任何代码区域。

help-repo = 查看或修改房间使用的仓库。
help-repo-show = 显示房间的仓库。
help-repo-set = 设置房间的仓库；URL 中的凭据会被移除。
help-repo-clear = 忘记房间的仓库。
repo-show = 房间使用 { $url }（分支 { $branch }）。
repo-none = 房间还没有仓库。用 polyph repo set <url> 设置。
connect-by-extension = { $client } 已通过 Polyph 扩展或插件使用 Polyph，无需更改。
doctor-client-extension = { $client } 通过 Polyph 扩展或插件使用 Polyph。
help-hook = 智能体编辑器的钩子（由 Claude Code 插件运行）。
help-hook-pre-edit = 在智能体编辑文件之前：若其他智能体已认领该文件则阻止编辑，否则为其认领。
