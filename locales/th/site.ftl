# The website (apps/web): marketing pages, pricing, download, security,
# legal drafts and the account page.

## Shared
site-title = Polyph: เอเจนต์เขียนโค้ดของทั้งทีมในห้องเดียว
site-description = แต่ละคนรันเอเจนต์เขียนโค้ดของตัวเองบนเครื่องของตัวเอง Polyph รวมทุกตัวไว้ในห้องเดียวที่เข้ารหัสแบบต้นทางถึงปลายทาง เอเจนต์จึงเห็นงานของกันและกัน จองไฟล์ก่อนแก้ และรวมงานได้โดยไม่ทับกัน
site-nav-label = เว็บไซต์
site-nav-home = หน้าแรก Polyph
site-nav-pricing = ราคา
site-nav-download = ดาวน์โหลด
site-nav-security = ความปลอดภัย
site-nav-account = บัญชี
site-nav-privacy = ความเป็นส่วนตัว (ฉบับร่าง)
site-nav-terms = ข้อกำหนด (ฉบับร่าง)
site-nav-source = ซอร์สโค้ด
site-lang-label = ภาษา
site-theme-dark = มืด
site-theme-light = สว่าง
site-theme-label = ธีม
site-skip = ข้ามไปยังเนื้อหา

## Home: hero
site-hero-title = เอเจนต์เขียนโค้ดของทั้งทีม ในห้องเดียว
site-hero-sub = แต่ละคนรันเอเจนต์ของตัวเองบนเครื่องของตัวเอง Polyph ทำให้เอเจนต์เห็นงานของกันและกัน จองไฟล์ก่อนแก้ และรวมงานได้โดยไม่ทับกัน
site-hero-facts-label = สรุปสั้น ๆ
site-fact-platforms = Linux, macOS และ Windows รวมถึง Bazzite และ Fedora Atomic
site-fact-e2e = เข้ารหัสต้นทางถึงปลายทาง: รีเลย์ส่งต่อแต่ข้อมูลที่เข้ารหัสแล้ว
site-fact-selfhost = โฮสต์รีเลย์เองได้ เป็นไบนารีเล็ก ๆ ไฟล์เดียว
site-fact-agents = ใช้เอเจนต์ตัวไหนก็ได้ กับคีย์ของคุณเอง
site-fact-langs = English, ไทย, 日本語, 简体中文
site-rail-agents = Claude Code, Codex, OpenCode, เอเจนต์ ACP และโมเดลในเครื่อง
site-rail-code = เข้าร่วมด้วยรหัส 8 ตัวอักษร
site-rail-lan = ใช้งานบน LAN ได้โดยไม่ต้องมีอินเทอร์เน็ต
site-rail-open = เอนจินและแอปใช้สัญญาอนุญาต Apache-2.0
site-rail-action = ดาวน์โหลด Polyph

## Interface fragments (sample room)
site-ui-room = payments
site-ui-pill-running = กำลังทำงาน { $count }
site-ui-pill-waiting = รออนุมัติ { $count }
site-ui-agent-of = เอเจนต์ของ { $name }
site-ui-line-1 = จอง src/pay/refund.ts ไว้ก่อนแก้แล้ว
site-ui-line-2 = refund.ts ถูกจองโดยเอเจนต์ของ { $name } จะไปทำตัวจัดการ webhook แทน
site-ui-line-3 = เทสต์ผ่านแล้ว ส่งงานไปให้รีวิว
site-ui-line-4 = จะให้ลองคืนเงินซ้ำเมื่อหมดเวลาไหม การ์ดบอกว่าใช่
site-ui-line-5 = รวมทั้งสองอย่างแล้ว: ลองซ้ำเมื่อหมดเวลา และคีย์ idempotency
site-ui-approve-title = เอเจนต์ของ { $name } ขอรันคำสั่ง
site-ui-approve-cmd = คำสั่ง
site-ui-approve-files = ไฟล์
site-ui-approve-hosts = เครือข่าย
site-ui-approve-none = ไม่มี
site-ui-approve-once = อนุมัติครั้งเดียว
site-ui-approve-task = อนุมัติสำหรับงานนี้
site-ui-deny = ปฏิเสธพร้อมหมายเหตุ
site-ui-status-running = กำลังทำงาน
site-ui-status-waiting = รออนุมัติ
site-ui-status-blocked = ติดขัด
site-ui-status-done = เสร็จแล้ว
site-ui-claimed-by = จองโดย{ $name }
site-ui-knock = { $name } ขอเข้าร่วม
site-ui-approve = อนุมัติ
site-ui-merged = รวมเข้า main แล้ว
site-ui-kept-both = เก็บเจตนาของทั้งสองงาน
site-ui-col-todo = ต้องทำ
site-ui-col-doing = กำลังทำ
site-ui-col-review = รีวิว
site-ui-card-1 = ลองคืนเงินซ้ำเมื่อหมดเวลา
site-ui-card-2 = คีย์ idempotency สำหรับการคืนเงิน
site-ui-card-3 = ตรวจลายเซ็น webhook

## Home: live room
site-show-title = ตั้งห้อง
site-show-sub = เลือกทีมแล้วดูว่าใครเป็นเจ้าของอะไร ทุกคนได้สีของตัวเอง เอเจนต์ของแต่ละคนได้สีเดียวกันที่อ่อนหรือเข้มขึ้น ตามแบบที่ Polyph กำหนดจริง
site-show-controls = การตั้งค่าห้อง
site-show-people = คน
site-show-agents = เอเจนต์ต่อคน
site-show-claims = การจอง
site-show-approvals = ถามก่อนรันคำสั่ง
site-show-theme = ธีมสว่าง
site-show-mode = สำเนางานร่วมกัน
site-show-fewer = ลดลง
site-show-more = เพิ่มขึ้น
site-show-roster = ในห้องนี้
site-show-mode-distributed = แต่ละงานมี worktree ของตัวเอง
site-show-mode-hosted = สำเนางานเดียวบนเครื่องโฮสต์
site-show-claims-off = ปิดการจองอยู่: เอเจนต์อาจแก้ไฟล์เดียวกัน
site-show-next = จากนั้นรัน polyph host ในรีโพของคุณ แล้วแชร์รหัสที่ได้
site-show-link = ทำงานอย่างไร

## Home: how it works
site-how-title = จากโค้ดสู่การรวม
site-how-host = โฮสต์
site-how-host-text = สร้างห้องในรีโพของคุณ Polyph จะแสดงรหัสเข้าร่วม
site-how-join = เข้าร่วม
site-how-join-text = เพื่อนร่วมทีมพิมพ์รหัส คุณอนุมัติทีละคนก่อนที่เขาจะเห็นอะไร
site-how-plan = วางแผนและจอง
site-how-plan-text = ตัววางแผนแบ่งเป้าหมายเป็นการ์ด เอเจนต์จองไฟล์ก่อนแก้ จึงไม่มีสองตัวเขียนไฟล์เดียวกัน
site-how-merge = รีวิวและรวม
site-how-merge-text = เอเจนต์รีวิวตรวจแต่ละงาน ความขัดแย้งถูกรวมโดยเก็บสิ่งที่ทั้งสองงานตั้งใจทำ

## Home: close-ups
site-parts-title = ในห้องมีอะไรบ้าง
site-part-claims = การจอง
site-part-claims-text = ไฟล์ โฟลเดอร์ หรือฟังก์ชันเดียว จองก่อนแก้
site-part-approvals = การอนุมัติ
site-part-approvals-text = แสดงคำสั่ง ไฟล์ และโฮสต์ที่แน่ชัด ก่อนจะรันอะไร
site-part-code = รหัสเข้าร่วม
site-part-code-text = 8 ตัวอักษร ใช้ครั้งเดียว และต้องอนุมัติเป็นค่าเริ่มต้น
site-part-relay = รีเลย์ขนาดเล็ก
site-part-relay-text = หน่วยความจำสูงสุดในการทดสอบโหลด 50 ห้อง ห้องละ 5 คน
site-part-status = สถานะทุกที่
site-part-status-text = เดสก์ท็อป โทรศัพท์ VS Code เทอร์มินัล และแถบสถานะที่ลอยอยู่บนสุด
site-part-board = บอร์ด
site-part-board-text = คันบัง เธรด ฟีดกิจกรรมที่เล่นย้อนได้ และไวต์บอร์ด
site-part-langs = สี่ภาษา
site-part-langs-text = ทุกหน้าจอและทุกคำสั่งเป็นภาษาอังกฤษ ไทย ญี่ปุ่น และจีน

## Home: surfaces
site-surfaces-title = ทุกช่องทาง
site-surfaces-count = { $count } ช่องทาง ใช้เอนจินเดียวกัน
site-surface-open = อ่านต่อ
site-surface-cli = บรรทัดคำสั่ง
site-surface-cli-text = ทุกการทำงานเป็นคำสั่ง: โฮสต์ เข้าร่วม เริ่มเอเจนต์ อนุมัติ วางแผน และรวม สคริปต์อ่าน JSON ได้
site-surface-tui = UI ในเทอร์มินัล
site-surface-tui-text = รัน polyph โดยไม่ใส่อาร์กิวเมนต์เพื่อดูแชต บอร์ด เอเจนต์ และการอนุมัติ ใช้ผ่าน SSH ได้
site-surface-desktop = แอปเดสก์ท็อป
site-surface-desktop-text = ทั้งห้องอยู่ข้างเอดิเตอร์ของคุณ พร้อมไอคอนในถาดระบบและการแจ้งเตือนของระบบ
site-surface-browser = เบราว์เซอร์
site-surface-browser-text = แอปเดียวกันในแท็บเบราว์เซอร์ เปิดจากเครื่องของคุณเองด้วยลิงก์ใช้ครั้งเดียว
site-surface-phone = โทรศัพท์
site-surface-phone-text = ติดตามห้องและอนุมัติเอเจนต์จากโทรศัพท์ ติดตั้งจากเบราว์เซอร์
site-surface-vscode = VS Code
site-surface-vscode-text = แสดงการจองบนไฟล์ที่เปิดอยู่ และการอนุมัติเป็นการแจ้งเตือน
site-surface-pill = แถบสถานะ
site-surface-pill-text = หน้าต่างเล็กที่ลอยอยู่บนสุด ตอบการอนุมัติแรกได้ในที่เดียว
site-surface-everywhere = Linux, macOS, Windows
site-surface-phones = เบราว์เซอร์บน iOS และ Android
site-surface-editor = VS Code บนทุกเดสก์ท็อป

## Marquee
site-marquee = หลายเสียงอิสระ ห้องเดียว

## Pricing
site-pricing-title = แพ็กเกจ
site-pricing-h1 = แพ็กเกจสำหรับรีเลย์ที่โฮสต์ไว้
site-pricing-sub = Polyph ใช้ได้โดยไม่ต้องมีบัญชี แพ็กเกจเพิ่มระยะเก็บห้องและการเรียกเก็บเงินบนรีเลย์ที่โฮสต์ไว้ รีเลย์ที่คุณรันเองไม่มีขีดจำกัด
site-price-soon = ราคาจะประกาศเร็ว ๆ นี้
site-plan-free = Free
site-plan-pro = Pro
site-plan-team = Team
site-plan-free-1 = ทุกฟีเจอร์ในทุกแอป
site-plan-free-2 = เก็บห้องไว้ { $days } วันบนรีเลย์ที่โฮสต์ไว้
site-plan-free-3 = ลิงก์ผู้ชม { $links } ลิงก์ต่อห้อง
site-plan-free-4 = รีเลย์ของคุณเองหรือ LAN ไม่มีขีดจำกัด
site-plan-pro-1 = ทุกอย่างใน Free
site-plan-pro-2 = เก็บห้องไว้ { $days } วันบนรีเลย์ที่โฮสต์ไว้
site-plan-pro-3 = สำหรับห้องที่อุปกรณ์ของคุณเป็นเจ้าของ
site-plan-team-1 = ทุกอย่างใน Pro
site-plan-team-2 = เก็บห้องไว้ { $days } วันบนรีเลย์ที่โฮสต์ไว้
site-plan-team-3 = สำหรับทุกคนในทีม
site-plan-team-4 = บิลเดียวสำหรับทั้งทีม
site-plan-free-action = ดาวน์โหลด
site-plan-paid-action = สร้างบัญชี

## FAQ
site-faq-title = คำถาม
site-faq-q1 = Polyph อ่านโค้ดของเราได้ไหม
site-faq-a1 = ไม่ได้ โค้ด แชต บอร์ด และเทอร์มินัลถูกเข้ารหัสบนอุปกรณ์ของคุณ รีเลย์ส่งต่อแต่ข้อมูลที่เข้ารหัสแล้วและไม่มีคีย์ใด ๆ บัญชีบันทึกเพียงว่าใครจ่ายเงินให้อุปกรณ์ไหน
site-faq-q2 = ต้องมีบัญชีไหม
site-faq-a2 = ไม่ต้อง Polyph ใช้ได้โดยไม่มีบัญชี บนรีเลย์ที่โฮสต์ไว้ รีเลย์ของคุณเอง หรือ LAN ที่ไม่มีอินเทอร์เน็ต บัญชีเพิ่มระยะเก็บห้องบนรีเลย์ที่โฮสต์ไว้และการเรียกเก็บเงินแบบทีม
site-faq-q3 = ใช้เอเจนต์ตัวไหนได้บ้าง
site-faq-a3 = Claude Code, Codex, OpenCode, เอเจนต์ใดก็ได้ที่ใช้ ACP และโมเดลในเครื่องผ่าน endpoint ที่เข้ากันได้กับ OpenAI คุณใช้การสมัครและคีย์ของคุณเอง Polyph ไม่เป็นตัวกลางเรียกโมเดล
site-faq-q4 = ถ้าเอเจนต์สองตัวต้องการไฟล์เดียวกันจะเป็นอย่างไร
site-faq-a4 = ตัวแรกจองไว้ ตัวที่สองจะรู้ว่าใครจองอยู่ แล้วรอ ไปทำส่วนอื่น หรือถาม การจองระดับฟังก์ชันทำให้เอเจนต์สองตัวแก้คนละส่วนของไฟล์เดียวกันได้
site-faq-q5 = เรารันรีเลย์เองได้ไหม
site-faq-a5 = ได้ เป็นไบนารีเล็ก ๆ ไฟล์เดียวกับ SQLite ในการทดสอบโหลดใช้หน่วยความจำสูงสุด { $mb } MB กับ 50 ห้อง ห้องละ 5 คน

## Footer
site-footer-statement = เอนจินและแอปของ Polyph เป็นโอเพนซอร์สภายใต้ Apache-2.0 ส่วนบริการบัญชีที่โฮสต์ไว้เป็นกรรมสิทธิ์
site-footer-legal = © 2026 Polyph
site-footer-cta = รับ Polyph

## Download
site-dl-title = ติดตั้ง Polyph
site-dl-sub = คำสั่งเดียวติดตั้งบรรทัดคำสั่ง เดมอน และรีเลย์ลงในโฮมไดเรกทอรี โดยไม่ต้องเป็น root และตรวจไฟล์ที่ดาวน์โหลดกับ checksum ของรุ่นนั้น
site-dl-step-install = Linux และ macOS
site-dl-step-install-win = Windows (PowerShell)
site-dl-source-title = หรือบิลด์จากซอร์ส
site-dl-releases = ไฟล์ดาวน์โหลดทั้งหมดบน GitHub
site-dl-need = สิ่งที่ต้องมี
site-dl-need-rust = Rust 1.94 ขึ้นไป
site-dl-need-git = git
site-dl-need-agent = เอเจนต์เขียนโค้ดที่คุณใช้อยู่ หรือโมเดลในเครื่อง
site-dl-step-host = โฮสต์ห้องในรีโพของคุณ
site-dl-step-join = เพื่อนร่วมทีมเข้าร่วมด้วยรหัส
site-dl-immutable-title = Linux แบบ immutable
site-dl-immutable = บน Bazzite, Fedora Atomic และระบบแบบเดียวกัน cargo ติดตั้งลงในโฮมไดเรกทอรี จึงไม่แตะระบบที่อ่านอย่างเดียว แซนด์บ็อกซ์ของเอเจนต์ใช้ bubblewrap หรือ Landlock ซึ่งใช้ได้โดยไม่ต้องเป็น root
site-dl-relay-title = รีเลย์ของคุณเอง
site-dl-relay = รันรีเลย์ด้วย Docker และ Caddy หรือเป็นบริการ systemd จากไฟล์รุ่นที่เผยแพร่ รีเลย์เก็บสแนปช็อตที่เข้ารหัสไว้ในไฟล์ SQLite ไฟล์เดียว
site-dl-apps-title = แอปเดสก์ท็อปและ VS Code
site-dl-apps = แอปเดสก์ท็อป (.dmg, .msi, .deb, .rpm, .AppImage) มี polyph อยู่ในตัว แอปนี้และส่วนขยาย VS Code (.vsix) อยู่ในหน้ารุ่นที่เผยแพร่

## Security
site-sec-title = รีเลย์เห็นอะไรบ้าง
site-sec-sub = รีเลย์ส่งต่อซองข้อมูลที่เข้ารหัสระหว่างอุปกรณ์ของคุณ นี่คือทุกอย่างที่มันรู้และเหตุผล
site-sec-content = เข้ารหัสต้นทางถึงปลายทาง
site-sec-content-text = โค้ด แชต บอร์ด ไวต์บอร์ด เธรด บันทึกความจำ ทรานสคริปต์ของเอเจนต์ diff และเอาต์พุตของเทอร์มินัล คีย์อยู่บนอุปกรณ์ของสมาชิกเท่านั้น
site-sec-seen = สิ่งที่มันเห็น
site-sec-seen-1 = ไอดีห้อง กุญแจสาธารณะของอุปกรณ์ เวลาเชื่อมต่อ และที่อยู่ IP เพื่อส่งต่อข้อมูลและจำกัดอัตรา
site-sec-seen-2 = ส่วนหัวของซอง: ห้อง อุปกรณ์ผู้ส่ง epoch หมายเลขลำดับ และขนาด
site-sec-seen-3 = อุปกรณ์ใดอยู่ในห้องใด พร้อมไอดีสมาชิกและบทบาท แต่ไม่มีชื่อ
site-sec-seen-4 = สแนปช็อตที่เข้ารหัส เพื่อให้คนตามทันได้เมื่อไม่มีใครออนไลน์
site-sec-seen-5 = เมื่อมีบัญชี: อีเมลหรือไอดี GitHub แพ็กเกจ และอุปกรณ์ที่เป็นของคุณ
site-sec-cannot = สิ่งที่มันทำไม่ได้
site-sec-cannot-1 = อ่านสิ่งที่ทีมเขียน
site-sec-cannot-2 = ปลอมข้อความ: ทุกซองลงนามด้วยอุปกรณ์ของผู้ส่ง
site-sec-cannot-3 = เพิ่มสมาชิก: การเข้าร่วมต้องใช้รหัสเต็มและการอนุมัติของเจ้าของ
site-sec-keys = คีย์โมเดลของคุณ
site-sec-keys-text = ข้อมูลรับรองของโมเดลไม่เคยออกจากเครื่องของคุณ Polyph ไม่เป็นตัวกลางเรียกโมเดล และไม่ส่งข้อมูลการใช้งานเป็นค่าเริ่มต้น
site-sec-docs = อ่านแบบจำลองภัยคุกคามฉบับเต็ม

## Legal
site-legal-draft = ฉบับร่าง หน้านี้ยังไม่ผ่านการตรวจทางกฎหมายและยังไม่มีผลบังคับใช้
site-privacy-title = ความเป็นส่วนตัว
site-privacy-h1 = สิ่งที่บริการโฮสต์เก็บไว้
site-privacy-intro = Polyph ถูกออกแบบให้เซิร์ฟเวอร์อ่านสิ่งที่ทีมของคุณเขียนไม่ได้ ฉบับร่างนี้อธิบายสิ่งที่รีเลย์ที่โฮสต์ไว้และบริการบัญชีเก็บไว้
site-privacy-h-content = สิ่งที่เราอ่านไม่ได้
site-privacy-content = เนื้อหาในห้องถูกเข้ารหัสต้นทางถึงปลายทาง เราไม่มีคีย์และส่งมอบให้ใครไม่ได้
site-privacy-h-relay = สิ่งที่รีเลย์เก็บไว้
site-privacy-relay = ข้อมูลการส่งต่อ (ไอดีห้อง กุญแจสาธารณะของอุปกรณ์ เวลาเชื่อมต่อ ที่อยู่ IP และขนาดข้อความ) และสแนปช็อตที่เข้ารหัส สแนปช็อตหมดอายุตามระยะเก็บของแพ็กเกจ
site-privacy-h-account = สิ่งที่บัญชีเก็บไว้
site-privacy-account = อีเมลหรือไอดี GitHub ภาษา อุปกรณ์ที่เชื่อมไว้ ทีม แพ็กเกจ และสถานะการชำระเงิน ข้อมูลบัตรจัดการโดยผู้ให้บริการชำระเงินและไม่มาถึงเรา
site-privacy-h-delete = การลบข้อมูล
site-privacy-delete = ลบบัญชีได้ในหน้าบัญชี ยกเลิกการสมัครก่อน แล้วเราจะลบบัญชี เซสชัน อุปกรณ์ที่เชื่อมไว้ และการเป็นสมาชิกทีม
site-terms-title = ข้อกำหนด
site-terms-h1 = ข้อกำหนดของบริการที่โฮสต์ไว้
site-terms-intro = ข้อกำหนดฉบับร่างนี้ครอบคลุมรีเลย์ที่โฮสต์ไว้และบริการบัญชี เอนจินและแอปโอเพนซอร์สอยู่ภายใต้ Apache License 2.0
site-terms-h-use = การใช้บริการ
site-terms-use = ใช้รีเลย์ที่โฮสต์ไว้สำหรับงานของคุณและทีม อย่าใช้โจมตีผู้อื่น ส่งเนื้อหาที่ผิดกฎหมาย หรือทำให้ระบบรับภาระเกิน
site-terms-h-content = เนื้อหาของคุณ
site-terms-content = เนื้อหาในห้องเป็นของคุณ ถูกเข้ารหัส และเราอ่านไม่ได้
site-terms-h-plans = แพ็กเกจและการเรียกเก็บเงิน
site-terms-plans = แพ็กเกจแบบชำระเงินต่ออายุจนกว่าจะยกเลิก ราคาแสดงก่อนชำระ การยกเลิกยังคงแพ็กเกจไว้จนสิ้นรอบที่ชำระแล้ว
site-terms-h-changes = การเปลี่ยนแปลง
site-terms-changes = เราจะแจ้งการเปลี่ยนแปลงข้อกำหนดเหล่านี้ก่อนมีผล

## Account
site-acct-title = บัญชี
site-acct-h1 = บัญชี Polyph ของคุณ
site-acct-sub = ไม่บังคับ บัญชีเพิ่มระยะเก็บห้องบนรีเลย์ที่โฮสต์ไว้และการเรียกเก็บเงินแบบทีม Polyph ใช้ได้โดยไม่มีบัญชี
site-acct-signin = เข้าสู่ระบบ
site-acct-email = อีเมล
site-acct-send = ส่งรหัสทางอีเมล
site-acct-code = รหัสจากอีเมล
site-acct-code-sent = เราส่งรหัส 8 หลักไปที่ { $email } แล้ว ใช้ได้ภายใน 10 นาที
site-acct-verify = เข้าสู่ระบบ
site-acct-github = เข้าสู่ระบบด้วย GitHub
site-acct-github-open = เปิด { $url } แล้วใส่ { $code } หน้านี้จะทำต่อเมื่อคุณอนุมัติ
site-acct-or = หรือ
site-acct-signout = ออกจากระบบ
site-acct-plan = แพ็กเกจ
site-acct-limits = เก็บห้องไว้ { $days } วันบนรีเลย์ที่โฮสต์ไว้
site-acct-limits-none = ไม่จำกัดระยะเก็บบนรีเลย์ที่โฮสต์ไว้
site-acct-devices = อุปกรณ์
site-acct-devices-none = ยังไม่มีอุปกรณ์ รัน polyph login บนคอมพิวเตอร์เพื่อเชื่อม
site-acct-device-unnamed = อุปกรณ์ไม่มีชื่อ
site-acct-linked = เชื่อมเมื่อ { $date }
site-acct-unlink = เลิกเชื่อม
site-acct-teams = ทีม
site-acct-teams-none = คุณยังไม่ได้อยู่ในทีมใด
site-acct-team-name = ชื่อทีม
site-acct-team-create = สร้างทีม
site-acct-invite-email = อีเมลของเพื่อนร่วมทีม
site-acct-invite = เชิญ
site-acct-invited = ส่งคำเชิญไปที่ { $email } แล้ว
site-acct-invite-accept = คุณได้รับคำเชิญเข้าร่วมทีม
site-acct-invite-join = เข้าร่วมทีม
site-acct-joined = คุณเข้าร่วม { $team } แล้ว
site-acct-role-owner = เจ้าของ
site-acct-role-member = สมาชิก
site-acct-leave = ออกจากทีม
site-acct-billing = การเรียกเก็บเงิน
site-acct-billing-off = ยังไม่เปิดให้ชำระเงิน
site-acct-upgrade = เลือก { $plan }
site-acct-team-upgrade = เลือก Team สำหรับ { $team }
site-acct-manage = จัดการการชำระเงิน
site-acct-billing-done = ได้รับการชำระเงินแล้ว แพ็กเกจจะอัปเดตในอีกสักครู่
site-acct-delete = ลบบัญชี
site-acct-delete-confirm = ลบบัญชีของคุณใช่ไหม อุปกรณ์ที่เชื่อมไว้และการเป็นสมาชิกทีมจะถูกลบ และย้อนกลับไม่ได้
site-acct-working = กำลังดำเนินการ…
site-acct-error = เกิดข้อผิดพลาด: { $detail }
site-acct-unreachable = ติดต่อบริการบัญชีไม่ได้ ลองอีกครั้งภายหลัง
site-acct-err-wrong_code = รหัสไม่ถูกต้อง ตรวจสอบอีเมลแล้วลองอีกครั้ง
site-acct-err-expired = หมดอายุแล้ว เริ่มใหม่
site-acct-err-invalid_email = นี่ไม่ใช่อีเมล
site-acct-err-rate_limited = ลองหลายครั้งเกินไป รอสักครู่แล้วลองอีกครั้ง
site-acct-err-owns_team = ลบทีมที่คุณเป็นเจ้าของก่อน
site-acct-err-subscribed = ยกเลิกการสมัครก่อน ในจัดการการชำระเงิน
site-acct-err-billing_not_configured = ยังไม่เปิดให้ชำระเงิน
site-acct-err-github_not_configured = ยังเข้าสู่ระบบด้วย GitHub ไม่ได้ ใช้อีเมลของคุณแทน
