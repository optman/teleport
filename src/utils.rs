use crate::teleport::TeleportInit;
use crate::teleport::{TeleportAction, TeleportEnc, TeleportFeatures, TeleportHeader};
use crate::*;
use byteorder::{LittleEndian, ReadBytesExt};
use rand::prelude::*;
use rand::{distributions::Alphanumeric, Rng};
use std::hash::Hasher;
use xxhash_rust::xxh3;

struct SizeUnit {
    value: f64,
    unit: char,
}

struct UpdateUnit {
    partial: SizeUnit,
    total: SizeUnit,
    percent: f64,
}

pub fn print_updates(received: f64, header: &TeleportInit) {
    let units = update_units(received as f64, header.filesize as f64);
    print!(
        "\r => {:>8.03}{} of {:>8.03}{} ({:02.02}%)",
        units.partial.value, units.partial.unit, units.total.value, units.total.unit, units.percent
    );
    io::stdout().flush().unwrap();
}

fn update_units(partial: f64, total: f64) -> UpdateUnit {
    let percent: f64 = (partial as f64 / total as f64) * 100f64;
    let p = identify_unit(partial);
    let t = identify_unit(total);

    UpdateUnit {
        partial: p,
        total: t,
        percent,
    }
}

fn identify_unit(mut value: f64) -> SizeUnit {
    let unit = ['B', 'K', 'M', 'G', 'T'];

    let mut count = 0;
    loop {
        if (value / 1024.0) > 1.0 {
            count += 1;
            value /= 1024.0;
        } else {
            break;
        }
        if count == unit.len() - 1 {
            break;
        }
    }

    SizeUnit {
        value,
        unit: unit[count],
    }
}

pub fn send_packet(
    sock: &mut TcpStream,
    action: TeleportAction,
    enc: &Option<TeleportEnc>,
    data: Vec<u8>,
) -> Result<(), Error> {
    let mut header = TeleportHeader::new(action);

    // If encryption is enabled
    if let Some(ctx) = enc {
        // Use random IV
        let mut rng = StdRng::from_entropy();
        let mut iv: [u8; 12] = [0; 12];
        rng.fill(&mut iv);

        header.action |= TeleportAction::Encrypted as u8;

        // Encrypt the data array
        header.data = ctx.encrypt(&iv, &data)?;

        // Set the IV in the header
        header.iv = Some(iv);
    } else {
        header.data = data;
    }

    // Serialize the message
    let message = header.serialize()?;

    // Send the packet
    sock.write_all(&message)?;
    sock.flush()?;

    Ok(())
}

pub fn recv_packet(
    sock: &mut TcpStream,
    dec: &Option<TeleportEnc>,
) -> Result<TeleportHeader, Error> {
    let mut initbuf: [u8; 13] = [0; 13];
    loop {
        let len = sock.peek(&mut initbuf)?;
        if len == 13 {
            break;
        }
    }

    let mut init: &[u8] = &initbuf;
    let protocol = init.read_u64::<LittleEndian>().unwrap();
    if protocol != PROTOCOL {
        return Err(Error::new(ErrorKind::InvalidData, "Invalid protocol"));
    }

    let packet_len = init.read_u32::<LittleEndian>().unwrap();
    let action = init.read_u8().unwrap();

    // Include IV size in length
    let mut total_len = 13 + packet_len as usize;
    let encrypted = action & TeleportAction::Encrypted as u8 == TeleportAction::Encrypted as u8;
    if encrypted {
        total_len += 12;
    }

    let mut buf = Vec::<u8>::new();
    buf.resize(total_len, 0);

    sock.read_exact(&mut buf)?;

    let mut out = TeleportHeader::new(TeleportAction::Init);
    out.deserialize(buf)?;

    if encrypted {
        out.action ^= TeleportAction::Encrypted as u8;
        if let Some(ctx) = dec {
            out.data = ctx.decrypt(&out.iv.unwrap(), &out.data)?;
        }
    }

    Ok(out)
}

fn gen_chunk_size(file_size: u64) -> usize {
    let mut chunk = 1024;
    loop {
        if file_size / chunk > 2048 {
            chunk *= 2;
        } else {
            break;
        }
    }

    if chunk > u32::MAX as u64 {
        u32::MAX as usize
    } else {
        chunk as usize
    }
}

pub fn add_feature(opt: &mut Option<u32>, add: TeleportFeatures) -> Result<(), Error> {
    if let Some(o) = opt {
        *o |= add as u32;
        *opt = Some(*o);
    } else {
        *opt = Some(add as u32);
    }

    Ok(())
}

pub fn check_feature(opt: &Option<u32>, check: TeleportFeatures) -> bool {
    if let Some(o) = opt {
        if o & check as u32 == check as u32 {
            return true;
        }
    }

    false
}

// Called from server
pub fn calc_delta_hash(mut file: &File) -> Result<teleport::TeleportDelta, Error> {
    let meta = file.metadata()?;
    let file_size = meta.len();

    file.seek(SeekFrom::Start(0))?;
    let mut buf = Vec::<u8>::new();
    buf.resize(gen_chunk_size(meta.len()), 0);
    let mut whole_hasher = xxh3::Xxh3::new();
    let mut chunk_hash = Vec::<u64>::new();

    loop {
        let mut hasher = xxh3::Xxh3::new();
        // Read a chunk of the file
        let len = match file.read(&mut buf) {
            Ok(l) => l,
            Err(s) => return Err(s),
        };
        if len == 0 {
            break;
        }

        hasher.write(&buf);
        chunk_hash.push(hasher.finish());

        whole_hasher.write(&buf);
    }

    let mut out = teleport::TeleportDelta::new();
    out.filesize = file_size as u64;
    out.chunk_size = buf.len().try_into().unwrap();
    out.hash = whole_hasher.finish();
    out.chunk_hash = chunk_hash;

    file.seek(SeekFrom::Start(0))?;

    Ok(out)
}

pub(crate) fn random_id() -> String {
    rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(7)
        .map(char::from)
        .collect()
}
