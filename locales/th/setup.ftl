# เชื่อมต่อเอเจนต์ของคุณเอง เริ่มต้นใช้งาน ตรวจการตั้งค่า และรีโพของห้อง
help-connect = เชื่อมต่อเอเจนต์ที่คุณใช้อยู่แล้ว (Claude Code, Codex, Cursor, VS Code, Gemini CLI, Windsurf, Zed, OpenCode) เข้ากับ Polyph
help-disconnect = นำ Polyph ออกจากการตั้งค่าของเอเจนต์
help-start = ตั้งค่า Polyph ในโฟลเดอร์นี้: เดมอน ชื่อของคุณ ห้อง รหัสเข้าร่วม และเอเจนต์ของคุณ
help-doctor = ตรวจการตั้งค่า และแสดงคำสั่งที่ใช้แก้แต่ละปัญหา
help-sync = ดึงงานล่าสุด และเลื่อนบรานช์ของคุณไปข้างหน้าเมื่อไม่มีอะไรสูญหาย

connect-no-home = ไม่พบโฟลเดอร์บ้านของคุณ ตั้งค่า HOME แล้วลองอีกครั้ง
connect-unknown = Polyph ไม่รู้จักเอเจนต์ "{ $name }" ที่รู้จัก: { $known }
connect-none-found = ไม่พบเอเจนต์ที่ Polyph เชื่อมต่อได้ในคอมพิวเตอร์นี้ ระบุชื่อเพื่อเชื่อมต่อได้เลย: { $known }
connect-create = { $client }: สร้าง { $file }
connect-change-in = { $client }: แก้ไข { $file }
connect-remove-from = { $client }: นำ Polyph ออกจาก { $file }
connect-drops-comments = ความคิดเห็นในไฟล์นี้จะไม่ถูกเก็บไว้ แต่ยังอยู่ในไฟล์สำรอง
connect-already = { $client } เชื่อมต่ออยู่แล้ว
connect-needs-yes = การแก้ไขการตั้งค่าของเอเจนต์ต้องยืนยันก่อน ให้รันในเทอร์มินัล หรือเพิ่ม --yes
connect-confirm = บันทึกการเปลี่ยนแปลงเหล่านี้หรือไม่? [y/N]
connect-cancelled = ไม่ได้เปลี่ยนแปลงอะไร
connect-done = เชื่อมต่อ { $client } แล้ว
connect-backup = เก็บไฟล์เดิมไว้ที่ { $file }
connect-write-failed = เขียน { $file } ไม่ได้: { $error }
connect-restart = เริ่มเอเจนต์เหล่านั้นใหม่ (หรือโหลดเซิร์ฟเวอร์ MCP ใหม่) เพื่อให้ใช้ Polyph ได้
disconnect-confirm = นำ Polyph ออกจากการตั้งค่าเหล่านี้หรือไม่? [y/N]
disconnect-done = ยกเลิกการเชื่อมต่อ { $client } แล้ว
disconnect-nothing = ไม่มีเอเจนต์ใดที่มี Polyph อยู่ในการตั้งค่า

start-ask-name = ชื่อของคุณที่เพื่อนร่วมทีมจะเห็น [{ $guess }]:
start-needs-name = Polyph ต้องรู้ชื่อของคุณก่อน: polyph config set name "ชื่อของคุณ"
start-name-set = คุณคือ { $name } ใน Polyph
start-room-existing = โฟลเดอร์นี้เป็นของห้อง { $name }
start-room-created = สร้างห้อง { $name } สำหรับ { $dir } แล้ว
start-ready = พร้อมแล้ว เอเจนต์ที่คุณเริ่มใน { $dir } จะเข้าห้อง { $room } ในชื่อ { $name }/<เอเจนต์>
start-teammates = เพื่อนร่วมทีมเข้าร่วมด้วย: polyph join { $code }

doctor-fix = วิธีแก้: { $command }
doctor-git-ok = ติดตั้ง git แล้ว
doctor-git-missing = ยังไม่ได้ติดตั้ง git ซึ่ง Polyph ต้องใช้สำหรับบรานช์ การรวมงาน และการโคลน
doctor-daemon-ok = เดมอนกำลังทำงาน (เวอร์ชัน { $version })
doctor-daemon-down = เดมอนไม่ได้ทำงาน
doctor-daemon-version = เดมอนใช้เวอร์ชัน { $daemon } แต่ polyph นี้เป็นเวอร์ชัน { $cli } ให้เริ่มเดมอนใหม่
doctor-name-ok = ชื่อของคุณคือ { $name }
doctor-name-missing = คุณยังไม่ได้ตั้งชื่อ
doctor-folder-unlinked = โฟลเดอร์นี้ยังไม่ได้ผูกกับห้อง เอเจนต์ที่เริ่มที่นี่จึงไม่รู้ว่าจะเข้าห้องไหน
doctor-folder-gone = โฟลเดอร์นี้ผูกกับ { $room } ซึ่งไม่อยู่ในคอมพิวเตอร์นี้แล้ว
doctor-room-ok = เชื่อมต่อกับห้อง { $room } แล้ว
doctor-room-closed = ห้อง { $room } ปิดอยู่ในคอมพิวเตอร์นี้
doctor-room-offline = ห้อง { $room } ติดต่อรีเลย์ไม่ได้ ({ $relay })
doctor-repo-ok = โฟลเดอร์นี้เป็นโคลนของ { $url }
doctor-repo-differs = origin ของโฟลเดอร์นี้คือ { $have } แต่ห้องทำงานกับ { $want }
doctor-repo-missing = ห้องทำงานกับ { $url } แต่โฟลเดอร์นี้ไม่ได้เป็นโคลนของรีโพนั้น
doctor-client-ok = เชื่อมต่อ { $client } แล้ว
doctor-client-unconnected = ติดตั้ง { $client } แล้ว แต่ยังไม่ได้เชื่อมต่อกับ Polyph
doctor-client-missing-exe = { $client } เรียกใช้ { $path } ซึ่งไม่มีอยู่
doctor-client-port = { $client } ชี้ไปที่ที่อยู่เว็บเก่า ตอนนี้เดมอนรับฟังที่พอร์ต { $port }
doctor-problems = { $count ->
   *[other] พบ { $count } ปัญหา
}
doctor-all-good = ทุกอย่างเรียบร้อยดี

repo-remembered = ห้องจำรีโพของตัวเองไว้แล้ว: { $url }
repo-linked = ผูก { $dir } กับห้องแล้ว
repo-clone-hint = ห้องนี้ทำงานกับ { $url } ให้โคลนรีโพนั้น แล้วรัน polyph start ในโฟลเดอร์ที่โคลนมา
repo-clone-confirm = โคลน { $url } ลงใน { $dir } ด้วยบัญชี git ของคุณหรือไม่? [Y/n]
repo-clone-partial = รีโพนี้มี { $files } ไฟล์ จะโคลนโดยยังไม่ดึงเนื้อหาไฟล์ git จะดาวน์โหลดเมื่อคุณต้องใช้
repo-clone-failed = โคลน { $url } ไม่ได้: { $error }
repo-cloned = โคลน { $url } ลงใน { $dir } และผูกกับห้องแล้ว

sync-nothing = ยังไม่มีโฟลเดอร์ที่ผูกกับห้องในเครื่องนี้ ให้รัน polyph start ในโฟลเดอร์งานของคุณ
sync-failed = { $dir }: { $error }
sync-no-upstream = { $dir }: { $branch } ไม่มีบรานช์ต้นทาง จึงไม่มีที่ให้เลื่อนไป
sync-updated = { $dir }: เลื่อน { $branch } ไปข้างหน้า { $count } คอมมิต
sync-held-dirty = { $dir }: { $branch } ตามหลัง { $count } คอมมิต แต่คุณมีการแก้ไขที่ยังไม่คอมมิต ให้คอมมิตหรือ stash ก่อน แล้วซิงก์อีกครั้ง
sync-held-diverged = { $dir }: { $branch } มีคอมมิตของตัวเอง { $ahead } คอมมิต และตามหลัง { $behind } คอมมิต ให้ merge หรือ rebase เอง
sync-ahead = { $dir }: { $branch } เป็นปัจจุบัน และมี { $count } คอมมิตที่ยังไม่ได้ push
sync-current = { $dir }: { $branch } เป็นปัจจุบันแล้ว

help-brief = อ่านหรือแก้สรุปโปรเจกต์ที่เอเจนต์ทุกตัวอ่าน: สถาปัตยกรรม ข้อตกลง และการตัดสินใจ
help-brief-show = แสดงสรุปโปรเจกต์
help-brief-set = แทนที่สรุปด้วยไฟล์ (- อ่านจากอินพุตมาตรฐาน)
help-brief-edit = แก้สรุปในโปรแกรมแก้ไขของคุณ
help-search = ค้นหาในโน้ต การ์ดงาน และเธรดของห้อง
help-catchup = งานที่เสร็จล่าสุด พร้อมสรุปจากเอเจนต์แต่ละตัว
help-areas = แสดงพื้นที่โค้ด (.polyph/AREAS หรือ CODEOWNERS) และผู้ดูแล

brief-by = แก้ไขล่าสุดโดย { $name }
brief-empty = ห้องนี้ยังไม่มีสรุปโปรเจกต์ เขียนได้ด้วย polyph brief edit ระหว่างนี้เอเจนต์จะอ่าน .polyph/brief.md ถ้ารีโพมีไฟล์นี้
brief-saved = บันทึกสรุปแล้ว เอเจนต์ทุกตัวในห้องอ่านได้ด้วย get_context
brief-unchanged = สรุปไม่มีการเปลี่ยนแปลง
brief-template = # สรุปโปรเจกต์
    
    ## สถาปัตยกรรม
    
    ## ข้อตกลง
    
    ## การตัดสินใจ
search-none = ไม่พบสิ่งที่ตรงกับ "{ $query }"
search-kind-note = โน้ต
search-kind-card = งาน
search-kind-thread = เธรด
catchup-brief = จากสรุปโปรเจกต์:
catchup-finished = เสร็จล่าสุด:
catchup-nothing = ยังไม่มีงานที่เสร็จ
areas-empty = รีโพนี้ยังไม่มีพื้นที่โค้ด เพิ่มไฟล์ .polyph/AREAS (รูปแบบเดียวกับ CODEOWNERS): แพตเทิร์น ตามด้วยผู้ดูแล
areas-from = พื้นที่โค้ดจาก { $file }:
areas-of = { $path } อยู่ใน { $pattern } ดูแลโดย { $owners }
areas-none-for = { $path } ไม่อยู่ในพื้นที่โค้ดใด

help-repo = ดูหรือเปลี่ยนรีโพที่ห้องทำงานอยู่
help-repo-show = แสดงรีโพของห้อง
help-repo-set = ตั้งรีโพของห้อง ข้อมูลรับรองใน URL จะถูกลบออก
help-repo-clear = ลืมรีโพของห้อง
repo-show = ห้องทำงานกับ { $url } (บรานช์ { $branch })
repo-none = ห้องยังไม่มีรีโพ ตั้งได้ด้วย polyph repo set <url>
connect-by-extension = { $client } ได้รับ Polyph จากส่วนขยายหรือปลั๊กอิน Polyph อยู่แล้ว จึงไม่ต้องแก้อะไร
doctor-client-extension = { $client } ได้รับ Polyph จากส่วนขยายหรือปลั๊กอิน Polyph
help-hook = ฮุกสำหรับโปรแกรมแก้ไขของเอเจนต์ (ปลั๊กอิน Claude Code เรียกใช้)
help-hook-pre-edit = ก่อนเอเจนต์แก้ไฟล์: บล็อกการแก้ไขถ้าเอเจนต์อื่นจองไฟล์ไว้ ไม่เช่นนั้นจองไฟล์ให้

# ติดต่อรีเลย์ไม่ได้เพราะอะไร (polyph status และ polyph doctor)
relay-error-last = ข้อผิดพลาดล่าสุดของรีเลย์: { $message }
relay-fix-certificate = ใบรับรองของรีเลย์ไม่ได้รับความเชื่อถือ หากพร็อกซีของบริษัทตรวจสอบ TLS ต้องใส่ CA ของพร็อกซีนั้นในที่เก็บความเชื่อถือของระบบ หรือในไฟล์ที่ระบุด้วย SSL_CERT_FILE
relay-fix-dns = หาที่อยู่ของรีเลย์ไม่พบ ตรวจสอบเครือข่ายและที่อยู่ด้วย polyph config get relay
relay-fix-refused = รีเลย์ปฏิเสธการเชื่อมต่อ ตรวจสอบที่อยู่ด้วย polyph config get relay และตรวจว่าไฟร์วอลล์อนุญาตพอร์ตขาออก 443
relay-fix-timeout = รีเลย์ไม่ตอบภายในเวลาที่กำหนด ตรวจสอบเครือข่าย หรือพร็อกซีที่บล็อกการเชื่อมต่อ WebSocket
relay-fix-http = รีเลย์ตอบกลับด้วยข้อผิดพลาด HTTP (อาจเป็นพร็อกซีหรือที่อยู่ผิด) ตรวจสอบ polyph config get relay
relay-fix-rejected = รีเลย์ปฏิเสธอุปกรณ์เครื่องนี้ ตรวจสอบ polyph config get relay และบันทึกของรีเลย์
relay-fix-other = ตรวจสอบที่อยู่ด้วย polyph config get relay แล้วดูข้อผิดพลาดเต็มในบันทึกของ daemon
