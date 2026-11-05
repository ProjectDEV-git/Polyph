# polyph update: replaces an install made with the install script by a newer release.
help-update = อัปเดต polyph เป็นรุ่นล่าสุด
update-unsupported = ยังไม่มีรุ่นสำเร็จรูปสำหรับระบบนี้ ให้สร้าง polyph จากซอร์สโค้ดแทน
update-not-archive = polyph ตัวนี้ ({ $path }) ไม่ได้ติดตั้งด้วยสคริปต์ติดตั้ง จึงอัปเดตตัวเองไม่ได้ ให้อัปเดตด้วยวิธีเดียวกับที่ติดตั้ง แอปเดสก์ท็อปและส่วนขยาย VS Code มี polyph ของตัวเองอยู่แล้ว
update-no-release = ไม่พบรุ่นที่เผยแพร่ที่ github.com/{ $repo }
update-no-build = รุ่นนี้ไม่มีไฟล์สำหรับระบบนี้ ({ $target })
update-unreachable = ตรวจหาอัปเดตไม่ได้: { $error }
update-available = มี polyph { $version } ให้อัปเดตแล้ว (คุณใช้ { $current }) สั่ง `polyph update` เพื่อติดตั้ง
update-current = polyph { $version } เป็นรุ่นล่าสุดแล้ว
update-downloading = กำลังดาวน์โหลด { $file }
update-checksum = ไฟล์ที่ดาวน์โหลดไม่ตรงกับค่าตรวจสอบ ({ $file }) จึงไม่ได้เปลี่ยนแปลงอะไร
update-failed = อัปเดตไฟล์ใน { $dir } ไม่ได้: { $error }
update-done = อัปเดต polyph { $from } → { $to } แล้ว
update-daemon-restarted = เดมอนเริ่มทำงานใหม่ด้วยรุ่นใหม่แล้ว
update-daemon-restart-failed = เดมอนหยุดอยู่ และจะเริ่มทำงานอีกครั้งเมื่อคุณสั่ง polyph ครั้งถัดไป
update-daemon-busy = เดมอนยังใช้รุ่นเดิมต่อไประหว่างที่เอเจนต์ { $count } ตัวกำลังทำงาน เมื่อเสร็จแล้วให้เริ่มใหม่: polyph daemon stop แล้ว polyph daemon start

# polyph uninstall
help-uninstall = ถอนการติดตั้ง polyph ออกจากเครื่องนี้
uninstall-not-archive = polyph ตัวนี้ ({ $path }) ไม่ได้ติดตั้งด้วยสคริปต์ติดตั้ง ให้ถอนด้วยวิธีเดียวกับที่ติดตั้ง
uninstall-needs-yes = การถอนการติดตั้ง polyph ต้องยืนยันก่อน ให้สั่งในเทอร์มินัล หรือเพิ่ม --yes
uninstall-what = จะลบ polyph, polyph-relay และไคลเอนต์เบราว์เซอร์ออกจาก { $dir } และหยุดเดมอนพร้อมเอเจนต์ที่กำลังทำงานอยู่
uninstall-purge-what = และจะลบข้อมูล Polyph ของเครื่องนี้ใน { $dir } ด้วย ได้แก่ตัวตนของอุปกรณ์ ห้อง และการตั้งค่า ห้องที่มีอยู่แค่ในเครื่องนี้จะหายไป
uninstall-confirm = ดำเนินการต่อไหม [y/N]
uninstall-cancelled = ไม่ได้ลบอะไร
uninstall-failed = ลบไฟล์ใน { $dir } ไม่ได้: { $error }
uninstall-done = ถอนการติดตั้ง polyph แล้ว
uninstall-kept-data = ข้อมูล Polyph ของคุณยังอยู่ใน { $dir } เผื่อติดตั้งใหม่ภายหลัง หากต้องการลบด้วย ให้ลบโฟลเดอร์นั้น
