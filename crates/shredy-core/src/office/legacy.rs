//! 旧形式 Office（.doc / .xls / .ppt — OLE Compound Document）の正規化抽出（仕様 §3.11）。
//!
//! - `.xls`: `calamine` でシート・セル値を TSV 収集（自作不要）。
//! - `.doc`: FIB → CLX → PlcfPcd ピーステーブルを走査して本文を再構成。
//! - `.ppt`: PowerPoint Document ストリームのレコードツリーを走査しテキストアトムを収集。
//!
//! 旧形式は可視テキストのみを抽出するため、`Summary Information` の最終保存日時等の
//! 揮発メタデータを自然に無視する。`.ppt` の高速保存（追記）・UserEdit チェーンは
//! 仕様記載のとおり未対応（ベストエフォート）。

use std::io::{Cursor, Read, Seek};

use crate::error::{CoreError, Result};

/// レコード走査の再帰深度上限（ループ・破損ガード）。
const PPT_MAX_DEPTH: usize = 32;

/// `.doc` ピーステーブルのピース数上限。正常な文書では十分に大きく、
/// 不正な PlcPcd（最大 256 MiB の Table ストリーム ≈ 2,200 万ピース）による
/// 過大な `Vec::with_capacity`（OOM/プロセス panic）を防ぐ（DoS 対策）。
const MAX_DOC_PIECES: usize = 1 << 20;

// ───────────────────────── .xls (calamine) ─────────────────────────

/// `.xls` からシート・セル値を TSV 収集し、辞書順ソートして連結する（仕様 §3.11）。
///
/// calamine は不正な `.xls`（負の SST 長など）に対し内部で `unwrap()` パニックを
/// 起こし得るため、`catch_unwind` でプロセス全体のクラッシュを防ぎ `Err` に変換する。
/// （`panic = "abort"` 時は捕捉できないが、本プロジェクトは既定の unwind を使う。）
pub fn extract_xls(bytes: &[u8]) -> Result<String> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| extract_xls_inner(bytes)))
        .unwrap_or_else(|_| {
            Err(CoreError::Office(
                "xls parse panicked (corrupt file)".into(),
            ))
        })
}

fn extract_xls_inner(bytes: &[u8]) -> Result<String> {
    use calamine::{Reader, Xls};

    // `Cursor<&[u8]>` は Read + Seek を満たすため to_vec() のフルコピーは不要。
    let cursor = Cursor::new(bytes);
    let mut workbook =
        Xls::new(cursor).map_err(|e| CoreError::Office(format!("xls open failed: {e}")))?;

    let mut lines: Vec<String> = Vec::new();
    let names = workbook.sheet_names().to_vec();
    for name in names {
        let range = workbook
            .worksheet_range(&name)
            .map_err(|e| CoreError::Office(format!("xls sheet {name}: {e}")))?;
        for (row, col, cell) in range.used_cells() {
            lines.push(format!("{name}\t{row}\t{col}\t{cell}"));
        }
    }
    // 保存時のセル走査順差異を辞書順で正規化する。
    lines.sort();
    if lines.is_empty() {
        return Err(CoreError::Office("xls has no cells".to_string()));
    }
    Ok(lines.join("\n"))
}

// ───────────────────────── .doc (FIB→CLX→PlcfPcd) ─────────────────────────

/// `.doc` から本文テキストを抽出する。
pub fn extract_doc(bytes: &[u8]) -> Result<String> {
    let mut comp = open_compound(bytes, "doc")?;
    let word = read_stream(&mut comp, "WordDocument")?;
    // FIB フラグ fWhichTblStm (0x0A の bit 0x0200) で Table ストリームを選ぶ。
    let flags = u16le(&word, 0x0A).ok_or_else(|| CoreError::Office("doc FIB too short".into()))?;
    // fWhichTblStm が立っていれば 1Table、無ければ 0Table。無い場合のみ他方へフォールバック
    // （同一ストリームの二重読みを避ける）。
    let table = if flags & 0x0200 != 0 {
        read_stream(&mut comp, "1Table").or_else(|_| read_stream(&mut comp, "0Table"))
    } else {
        read_stream(&mut comp, "0Table").or_else(|_| read_stream(&mut comp, "1Table"))
    }?;
    let raw = doc_text_from_streams(&word, &table)?;
    Ok(strip_doc_controls(&raw))
}

/// WordDocument / Table ストリームのバイト列から本文を再構成する（テスト容易化のため分離）。
fn doc_text_from_streams(word: &[u8], table: &[u8]) -> Result<String> {
    if u16le(word, 0) != Some(0xA5EC) {
        return Err(CoreError::Office("not a Word .doc (bad wIdent)".into()));
    }
    let fc_clx =
        u32le(word, 0x01A2).ok_or_else(|| CoreError::Office("doc FIB no fcClx".into()))? as usize;
    let lcb_clx =
        u32le(word, 0x01A6).ok_or_else(|| CoreError::Office("doc FIB no lcbClx".into()))? as usize;
    let clx = table
        .get(fc_clx..fc_clx.checked_add(lcb_clx).unwrap_or(table.len()))
        .ok_or_else(|| CoreError::Office("doc CLX out of range".into()))?;

    let pieces = parse_piece_table(clx)?;
    // ピースの char_count 合計を容量ヒントにして push_str の再確保を抑える。
    let cap = pieces.iter().map(|p| p.char_count).sum();
    let mut text = String::with_capacity(cap);
    for piece in &pieces {
        text.push_str(&decode_piece(word, piece)?);
    }
    Ok(text)
}

/// 1 テキストピース（CP 範囲とストリーム内オフセット）。
#[derive(Debug, Clone, Copy)]
struct Piece {
    char_count: usize,
    /// WordDocument ストリーム内のバイトオフセット。
    byte_offset: usize,
    /// 圧縮（CP1252 1byte/char）か、非圧縮（UTF-16LE 2byte/char）か。
    compressed: bool,
}

/// CLX を走査して PlcfPcd（ピーステーブル）を取り出す（[MS-DOC] 2.9.38 相当）。
fn parse_piece_table(clx: &[u8]) -> Result<Vec<Piece>> {
    let mut pos = 0;
    while pos < clx.len() {
        match clx[pos] {
            // Prc: clxt(1) + cbGrpprl(2) + GrpPrl[cbGrpprl] を読み飛ばす。
            0x01 => {
                let cb = u16le(clx, pos + 1)
                    .ok_or_else(|| CoreError::Office("doc CLX Prc truncated".into()))?
                    as usize;
                pos += 3 + cb;
            }
            // Pcdt: clxt(1) + lcb(4) + PlcPcd[lcb]。
            0x02 => {
                let lcb = u32le(clx, pos + 1)
                    .ok_or_else(|| CoreError::Office("doc CLX Pcdt truncated".into()))?
                    as usize;
                let start = pos + 5;
                let plc = clx
                    .get(start..start.checked_add(lcb).unwrap_or(clx.len()))
                    .ok_or_else(|| CoreError::Office("doc PlcPcd out of range".into()))?;
                return parse_plcpcd(plc);
            }
            other => {
                return Err(CoreError::Office(format!(
                    "doc CLX unknown clxt {other:#x}"
                )));
            }
        }
    }
    Err(CoreError::Office("doc CLX has no Pcdt".into()))
}

/// PlcPcd（CP 配列 + PCD 配列）からピース列を構築する。
fn parse_plcpcd(plc: &[u8]) -> Result<Vec<Piece>> {
    // lcb = (n+1)*4 + n*8 = 4 + 12n → n = (len-4)/12。
    if plc.len() < 4 + 12 {
        return Ok(Vec::new());
    }
    let n = (plc.len() - 4) / 12;
    // 不正な PlcPcd による過大確保（OOM）を防ぐ（DoS 対策）。
    if n > MAX_DOC_PIECES {
        return Err(CoreError::Office(format!(
            "doc PlcPcd piece count {n} exceeds limit {MAX_DOC_PIECES}"
        )));
    }
    let pcd_base = (n + 1) * 4;
    let mut pieces = Vec::with_capacity(n);
    for i in 0..n {
        let cp0 = u32le(plc, i * 4).unwrap_or(0) as usize;
        let cp1 = u32le(plc, (i + 1) * 4).unwrap_or(0) as usize;
        let char_count = cp1.saturating_sub(cp0);
        let fc_raw = u32le(plc, pcd_base + i * 8 + 2)
            .ok_or_else(|| CoreError::Office("doc PCD truncated".into()))?;
        let compressed = fc_raw & 0x4000_0000 != 0;
        let fc = (fc_raw & 0x3FFF_FFFF) as usize;
        let byte_offset = if compressed { fc / 2 } else { fc };
        pieces.push(Piece {
            char_count,
            byte_offset,
            compressed,
        });
    }
    Ok(pieces)
}

/// ピースを WordDocument ストリームからデコードする。
fn decode_piece(word: &[u8], piece: &Piece) -> Result<String> {
    if piece.char_count == 0 {
        return Ok(String::new());
    }
    if piece.compressed {
        let end = piece.byte_offset.saturating_add(piece.char_count);
        let bytes = word
            .get(piece.byte_offset..end)
            .ok_or_else(|| CoreError::Office("doc piece (8-bit) out of range".into()))?;
        Ok(decode_cp1252(bytes))
    } else {
        let end = piece
            .byte_offset
            .saturating_add(piece.char_count.saturating_mul(2));
        let bytes = word
            .get(piece.byte_offset..end)
            .ok_or_else(|| CoreError::Office("doc piece (utf16) out of range".into()))?;
        Ok(decode_utf16le(bytes))
    }
}

/// Word 制御文字を整理する。フィールド命令（0x13…0x14）を除去し、段落等を空白へ。
fn strip_doc_controls(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut in_field_instruction = false;
    for ch in raw.chars() {
        match ch as u32 {
            0x13 => in_field_instruction = true,        // フィールド開始
            0x14 => in_field_instruction = false,       // セパレータ（以降が結果）
            0x15 => {}                                  // フィールド終了
            _ if in_field_instruction => {}             // 命令テキストは除外
            0x07 | 0x0B | 0x0C | 0x0D => out.push(' '), // セル/改行/改ページ → 空白
            c if c < 0x20 => {}                         // その他制御は除去
            _ => out.push(ch),
        }
    }
    out
}

// ───────────────────────── .ppt (record tree) ─────────────────────────

/// `.ppt` の PowerPoint Document ストリームからテキストを抽出する。
pub fn extract_ppt(bytes: &[u8]) -> Result<String> {
    let mut comp = open_compound(bytes, "ppt")?;
    let stream = read_stream(&mut comp, "PowerPoint Document")?;
    let mut texts = Vec::new();
    ppt_walk_records(&stream, 0, &mut texts);
    if texts.is_empty() {
        return Err(CoreError::Office("ppt has no text atoms".into()));
    }
    // 保存時のレコード順差異を辞書順ソートで正規化する（仕様 §3.11）。
    texts.sort();
    Ok(texts.join("\n"))
}

/// レコードツリーを走査し、テキストアトムを収集する。
fn ppt_walk_records(stream: &[u8], depth: usize, out: &mut Vec<String>) {
    if depth > PPT_MAX_DEPTH {
        return;
    }
    let mut pos = 0;
    while pos + 8 <= stream.len() {
        let ver_inst = match u16le(stream, pos) {
            Some(v) => v,
            None => break,
        };
        let rec_type = u16le(stream, pos + 2).unwrap_or(0);
        let rec_len = u32le(stream, pos + 4).unwrap_or(0) as usize;
        let data_start = pos + 8;
        let data_end = match data_start.checked_add(rec_len) {
            Some(e) if e <= stream.len() => e,
            _ => break, // 破損・範囲外で打ち切り。
        };
        let data = &stream[data_start..data_end];
        let rec_ver = ver_inst & 0x000F;
        if rec_ver == 0x000F {
            // コンテナ → 子レコードへ再帰。
            ppt_walk_records(data, depth + 1, out);
        } else {
            match rec_type {
                0x0FA0 => out.push(decode_utf16le(data)), // TextCharsAtom
                0x0FA8 => out.push(decode_cp1252(data)),  // TextBytesAtom
                _ => {}
            }
        }
        pos = data_end;
    }
}

// ───────────────────────── 共通ヘルパ ─────────────────────────

/// OLE 複合ドキュメントを開く（`label` はエラーメッセージ用の形式名: "doc" / "ppt"）。
fn open_compound<'a>(bytes: &'a [u8], label: &str) -> Result<cfb::CompoundFile<Cursor<&'a [u8]>>> {
    cfb::CompoundFile::open(Cursor::new(bytes))
        .map_err(|e| CoreError::Office(format!("{label} OLE open failed: {e}")))
}

/// OLE ストリームを上限付きで全読みする（偽装ストリームサイズによるメモリ枯渇を防ぐ）。
fn read_stream<R: Read + Seek>(comp: &mut cfb::CompoundFile<R>, name: &str) -> Result<Vec<u8>> {
    let stream = comp
        .open_stream(name)
        .map_err(|e| CoreError::Office(format!("OLE stream {name}: {e}")))?;
    let mut buf = Vec::new();
    stream
        .take(super::OFFICE_READ_CAP)
        .read_to_end(&mut buf)
        .map_err(|e| CoreError::Office(format!("OLE read {name}: {e}")))?;
    Ok(buf)
}

fn u16le(b: &[u8], off: usize) -> Option<u16> {
    let s = b.get(off..off + 2)?;
    Some(u16::from_le_bytes([s[0], s[1]]))
}

fn u32le(b: &[u8], off: usize) -> Option<u32> {
    let s = b.get(off..off + 4)?;
    Some(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}

/// UTF-16LE バイト列をデコードする（不正サロゲートは置換文字）。
fn decode_utf16le(bytes: &[u8]) -> String {
    // 中間 Vec<u16> を介さず、イテレータを直接 decode_utf16 へ渡す。
    let units = bytes
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]));
    char::decode_utf16(units)
        .map(|r| r.unwrap_or('\u{FFFD}'))
        .collect()
}

/// CP1252 バイト列をデコードする。equality 比較が目的のため Latin-1 近似で十分（design §4.4）。
fn decode_cp1252(bytes: &[u8]) -> String {
    bytes.iter().map(|&b| b as char).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 1 ピース（圧縮 CP1252）の .doc 用 WordDocument/Table バイト列を手構築する。
    fn build_doc_streams(text: &str) -> (Vec<u8>, Vec<u8>) {
        let text_offset = 0x200usize;
        let mut word = vec![0u8; text_offset + text.len()];
        // FIB: wIdent=0xA5EC, flags=0（0Table 使用）。
        word[0..2].copy_from_slice(&0xA5ECu16.to_le_bytes());
        // テキスト本体（圧縮 = 1byte/char）。
        word[text_offset..text_offset + text.len()].copy_from_slice(text.as_bytes());

        // PlcPcd: 1 ピース。CP[0]=0, CP[1]=char_count。
        let char_count = text.chars().count() as u32;
        let mut plc = Vec::new();
        plc.extend_from_slice(&0u32.to_le_bytes()); // CP[0]
        plc.extend_from_slice(&char_count.to_le_bytes()); // CP[1]
                                                          // PCD: flags(2)=0, FcCompressed(4), prm(2)=0。
        plc.extend_from_slice(&0u16.to_le_bytes());
        let fc = (text_offset as u32) * 2; // 圧縮: byteOffset = fc/2
        let fc_compressed = fc | 0x4000_0000;
        plc.extend_from_slice(&fc_compressed.to_le_bytes());
        plc.extend_from_slice(&0u16.to_le_bytes());

        // CLX: Pcdt = clxt(0x02) + lcb(u32) + PlcPcd。
        let mut clx = Vec::new();
        clx.push(0x02);
        clx.extend_from_slice(&(plc.len() as u32).to_le_bytes());
        clx.extend_from_slice(&plc);

        // Table ストリーム: 先頭に CLX を置く（fcClx=0）。
        let table = clx.clone();
        let fc_clx = 0u32;
        let lcb_clx = table.len() as u32;
        word[0x01A2..0x01A6].copy_from_slice(&fc_clx.to_le_bytes());
        word[0x01A6..0x01AA].copy_from_slice(&lcb_clx.to_le_bytes());
        (word, table)
    }

    #[test]
    fn doc_reconstructs_compressed_text() {
        let (word, table) = build_doc_streams("Hello World");
        let text = doc_text_from_streams(&word, &table).unwrap();
        assert_eq!(text, "Hello World");
    }

    #[test]
    fn doc_strips_field_instructions() {
        // 0x13 命令 0x14 結果 0x15 → 結果のみ残る。
        let raw = "See \u{13}HYPERLINK foo\u{14}the link\u{15} now";
        assert_eq!(strip_doc_controls(raw), "See the link now");
    }

    #[test]
    fn doc_paragraph_marks_become_spaces() {
        let raw = "line1\rline2\u{07}cell";
        assert_eq!(strip_doc_controls(raw), "line1 line2 cell");
    }

    #[test]
    fn doc_rejects_bad_ident() {
        let word = vec![0u8; 0x200];
        assert!(doc_text_from_streams(&word, &[]).is_err());
    }

    #[test]
    fn plcpcd_rejects_excessive_piece_count() {
        // ピース数が上限を超える不正な PlcPcd は、過大確保(OOM)前に Err を返す（DoS 対策）。
        let plc = vec![0u8; 4 + 12 * (MAX_DOC_PIECES + 16)];
        let err = parse_plcpcd(&plc);
        assert!(err.is_err(), "上限超過のピース数は Err になるべき");
    }

    #[test]
    fn xls_garbage_is_err_not_panic() {
        // 不正バイト列は panic させず Err を返す（calamine 内部 panic は catch_unwind が捕捉）。
        assert!(extract_xls(b"not a real xls file").is_err());
    }

    #[test]
    fn doc_full_ole_round_trip() {
        // cfb writer で OLE を合成し extract_doc を end-to-end 検証する。
        let (word, table) = build_doc_streams("Roundtrip");
        let mut buf = Vec::new();
        {
            let cursor = Cursor::new(&mut buf);
            let mut comp = cfb::CompoundFile::create(cursor).unwrap();
            use std::io::Write;
            comp.create_stream("WordDocument")
                .unwrap()
                .write_all(&word)
                .unwrap();
            comp.create_stream("0Table")
                .unwrap()
                .write_all(&table)
                .unwrap();
            comp.flush().unwrap();
        }
        assert_eq!(extract_doc(&buf).unwrap(), "Roundtrip");
    }

    /// PowerPoint レコード（ヘッダ8byte + データ）を組み立てる。
    fn ppt_record(ver_inst: u16, rec_type: u16, data: &[u8]) -> Vec<u8> {
        let mut r = Vec::new();
        r.extend_from_slice(&ver_inst.to_le_bytes());
        r.extend_from_slice(&rec_type.to_le_bytes());
        r.extend_from_slice(&(data.len() as u32).to_le_bytes());
        r.extend_from_slice(data);
        r
    }

    #[test]
    fn ppt_collects_text_bytes_atom() {
        let stream = ppt_record(0x0000, 0x0FA8, b"Slide text");
        let mut out = Vec::new();
        ppt_walk_records(&stream, 0, &mut out);
        assert_eq!(out, vec!["Slide text".to_string()]);
    }

    #[test]
    fn ppt_recurses_into_containers_and_sorts() {
        // コンテナ（recVer=0xF）が 2 つの TextBytesAtom を内包する。
        let mut children = Vec::new();
        children.extend(ppt_record(0x0000, 0x0FA8, b"Zebra"));
        children.extend(ppt_record(0x0000, 0x0FA8, b"Apple"));
        let container = ppt_record(0x000F, 0x03E8, &children);

        let mut out = Vec::new();
        ppt_walk_records(&container, 0, &mut out);
        out.sort();
        assert_eq!(out, vec!["Apple".to_string(), "Zebra".to_string()]);
    }

    #[test]
    fn ppt_text_chars_atom_utf16() {
        let mut data = Vec::new();
        for u in "あ".encode_utf16() {
            data.extend_from_slice(&u.to_le_bytes());
        }
        let stream = ppt_record(0x0000, 0x0FA0, &data);
        let mut out = Vec::new();
        ppt_walk_records(&stream, 0, &mut out);
        assert_eq!(out, vec!["あ".to_string()]);
    }

    #[test]
    fn ppt_truncated_record_is_ignored() {
        // recLen がストリーム長を超える → 打ち切り（パニックしない）。
        let mut stream = ppt_record(0x0000, 0x0FA8, b"hi");
        stream[4] = 0xFF; // recLen を巨大化
        let mut out = Vec::new();
        ppt_walk_records(&stream, 0, &mut out);
        assert!(out.is_empty());
    }

    #[test]
    fn utf16le_and_cp1252_decode() {
        assert_eq!(decode_utf16le(&[0x41, 0x00, 0x42, 0x00]), "AB");
        assert_eq!(decode_cp1252(&[0x41, 0x42, 0x43]), "ABC");
    }
}
