use std::fs::{self, DirEntry};
use ndarray::{Array, ArrayBase, OwnedRepr, Dim};

struct Header {
    format: String,
    version: String,
    axes: u8,
    res_x: f32,
    res_y: f32,
    samples_x: u16,
    samples_y: u16,
    sub_sets: Vec<SubSet>,
    channels: u8,
    samples: u32
}

struct SubSet {
    name: String,
    element_size: u8,
    sample_nums: u32
}

pub struct UsData {
    header: Header,
    datasets: Vec<ArrayBase<OwnedRepr<i16>, Dim<[usize; 3]>>>
}

impl UsData {
    pub fn load_sonoware(path: DirEntry) -> UsData {
        parse_file(path)
    }

    pub fn get_channel(&self, channel: usize) -> &ArrayBase<OwnedRepr<i16>, Dim<[usize; 3]>> {
        &self.datasets[channel]
    }

    pub fn c_scan(&self, channel: usize) -> ArrayBase<OwnedRepr<i16>, Dim<[usize; 2]>> {
        let data = &self.datasets.get(channel);

        match data {
            Some(array) => {
                let shape = array.shape();

                let mut scan: ArrayBase<OwnedRepr<i16>, Dim<[usize; 2]>> = Array::zeros((shape[0], shape[1]));

                for (row_index, row) in array.outer_iter().enumerate() {
                    for (col_index, col) in row.outer_iter().enumerate() {
                        let maximum = col.iter().max().unwrap();
                        
                        scan[[row_index, col_index]] = *maximum;
                    }
                }

                scan
            }

            None => {
                panic!("Invalid channel request");
            }
        }
    }
}

fn parse_file(file: DirEntry) -> UsData {
    let data = fs::read(file.path());

    match data {
        Ok(binary_data) => {
            let string_data = String::from_utf8_lossy(binary_data.as_slice());
            let header_ending = "|^Data Set^|";

            let header_end = string_data.find(header_ending);

            match header_end {
                Some(index) => {
                    let header_string = String::from_utf8(binary_data[..index].to_vec()).unwrap();
                    let header = parse_header(header_string);

                    let mut us_data = UsData {
                        header,
                        datasets: vec![]
                    };

                    let samples_x = &us_data.header.samples_x;
                    let samples_y = &us_data.header.samples_y;
                    let subsets = &us_data.header.sub_sets;

                    let mut data_bytes = binary_data[index + header_ending.len()..].iter().collect::<Vec<_>>();
                    
                    let points = *samples_x as u32 * *samples_y as u32;

                    for subset in subsets {
                        let values = subset.element_size as u32 * subset.sample_nums * points;
                        let sub_sample = data_bytes[..values as usize].to_vec();
                        data_bytes.drain(0..values as usize);

                        if subset.name.contains("Data") {
                            let sub_data = get_raw_data(sub_sample, &subset, *samples_x, *samples_y);

                            us_data.datasets.push(sub_data);
                        }
                    }

                    us_data
                }
                None => {
                    UsData {
                        header: None.unwrap(),
                        datasets: None.unwrap()
                    }
                }
            }
        }
        Err(error) => {
            println!("Error: {}", error);
            panic!("Failed to parse file");
        }
    }
}

fn parse_header(header: String) -> Header {
    let lines = header.lines().collect::<Vec<_>>();
    let format = get_entry(lines[0]);
    let version = get_entry(lines[1]);
    let axes = get_entry(lines[3]).parse::<u8>().unwrap();
    let subsets = get_entry(lines[4]).parse::<u8>().unwrap();
    let res_x = get_float_entry(lines[8]);
    let res_y = get_float_entry(lines[12]);
    let samples_x = parse_entry::<u16>(lines[6]);
    let samples_y = parse_entry::<u16>(lines[10]);

    let mut sub_sets = Vec::<SubSet>::new();

    let mut samples = 0;

    for i in 0..subsets {
        let skip = i * 12;

        sub_sets.push(SubSet { 
            name: get_entry(lines[14 + skip as usize]), 
            element_size: parse_entry::<u8>(lines[15 + skip as usize]), 
            sample_nums: parse_entry::<u32>(lines[17 + skip as usize])
        });

        if sub_sets.last().unwrap().sample_nums > samples {
            samples = sub_sets.last().unwrap().sample_nums;
        }
    }

    let channels = sub_sets.iter().filter(|&n| (*n).name.contains("Data")).count() as u8;

    Header { 
        format, 
        version, 
        axes,
        res_x, 
        res_y, 
        samples_x, 
        samples_y, 
        sub_sets, 
        channels,
        samples 
    }
}

fn get_entry(line: &str) -> String {
    match String::from(line).split(": ").last() {
        Some(value) => { String::from(value) }
        None => { String::from("") }
    }
}

fn parse_entry<T>(line: &str) -> T where T: std::str::FromStr {
    let value_string = get_entry(line);

    match value_string.parse::<T>() {
        Ok(value) => { value }
        Err(_) => {
            println!("Error while parsing: {}", value_string);
            panic!("Parsing header failed");
        }
    }
}

fn get_float_entry(line: &str) -> f32 {
    let string_value = get_entry(line);

    match string_value[..string_value.len() - 3].parse::<f32>() {
        Ok(value) => { value }
        Err(error) => {
            println!("Error: {}", error);
            panic!("Parsing header failed!");
        }
    }
}

fn get_raw_data(data: Vec<&u8>, sub_set: &SubSet, x: u16, y: u16) -> ArrayBase<OwnedRepr<i16>, Dim<[usize; 3]>> {    
    let mut array: ArrayBase<OwnedRepr<i16>, Dim<[usize; 3]>> = Array::zeros((y as usize, x as usize, sub_set.sample_nums as usize));
    
    let mut i = 0;

    for chunk in data.chunks(sub_set.element_size as usize) {
        let mut bytes: [u8; 2] = [0, 0];

        if chunk.len() == 1 {
            bytes[1] = *chunk[0];
        }
        else {
            bytes[0] = *chunk[0];
            bytes[1] = *chunk[1];
        }

        let sample = i % sub_set.sample_nums;
        let col = (i / sub_set.sample_nums) % x as u32;
        let row = i / (sub_set.sample_nums * x as u32);

        let value = i16::from_le_bytes(bytes);

        array[[row as usize, col as usize, sample as usize]] = value;
        i += 1;
    }

    array
}
