# polyph update: replaces an install made with the install script by a newer release.
help-update = 将 polyph 更新到最新版本。
update-unsupported = 此系统没有发布版构建，请从源码构建 polyph。
update-not-archive = 这个 polyph（{ $path }）不是用安装脚本安装的，无法自行更新。请按安装时的方式更新；桌面应用和 VS Code 扩展自带 polyph。
update-no-release = 在 github.com/{ $repo } 上找不到发布版本。
update-no-build = 此版本没有适用于此系统（{ $target }）的构建。
update-unreachable = 无法检查更新：{ $error }
update-available = polyph { $version } 已发布（当前为 { $current }）。运行 `polyph update` 进行安装。
update-current = polyph { $version } 已是最新版本。
update-downloading = 正在下载 { $file }
update-checksum = 下载的文件与校验和不符（{ $file }），未做任何更改。
update-failed = 无法更新 { $dir } 中的文件：{ $error }
update-done = 已将 polyph 从 { $from } 更新到 { $to }。
update-daemon-restarted = 守护进程已用新版本重新启动。
update-daemon-restart-failed = 守护进程已停止，下次运行 polyph 命令时会再次启动。
update-daemon-busy = 有 { $count } 个智能体正在工作，守护进程继续运行旧版本。完成后请重启：先 polyph daemon stop，再 polyph daemon start。

# polyph uninstall
help-uninstall = 从这台电脑上移除 polyph。
uninstall-not-archive = 这个 polyph（{ $path }）不是用安装脚本安装的；请按安装时的方式移除。
uninstall-needs-yes = 卸载 polyph 需要确认；请在终端中运行，或加上 --yes。
uninstall-what = 将从 { $dir } 移除 polyph、polyph-relay 和浏览器客户端，并停止守护进程及其中正在运行的智能体。
uninstall-purge-what = 同时会删除 { $dir } 中此设备的 Polyph 数据：设备身份、房间和设置。只保存在此设备上的房间将会丢失。
uninstall-confirm = 继续吗？[y/N]
uninstall-cancelled = 未移除任何内容。
uninstall-failed = 无法移除 { $dir } 中的文件：{ $error }
uninstall-done = polyph 已卸载。
uninstall-kept-data = 你的 Polyph 数据仍保留在 { $dir }，以便日后重新安装。如需一并删除，请删除该文件夹。
