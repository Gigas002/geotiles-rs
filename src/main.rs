use std::error::Error;

use gdal::Dataset;

pub fn resolution(z: i32) -> f64 {
    let res_factor = 180.0 / 256.0;

    return res_factor / 2f64.powi(z);
}

pub fn to_pixel_coordinate(x: f64, y: f64, z: i32) -> (f64, f64){
    let resolution = resolution(z);
    let pixel_x = (180.0 + x) / resolution;
    let pixel_y = (90.0 + y) / resolution;

    return (pixel_x, pixel_y);
}

pub fn to_number(x: f64, y: f64, z: i32) -> (i32, i32) {
    let pixel_coordinate = to_pixel_coordinate(x, y, z);

    let tile_x = ((pixel_coordinate.0 / 256.0).ceil() - 1.0) as i32;
    let tile_y = ((pixel_coordinate.1 / 256.0).ceil() - 1.0) as i32;

    return (tile_x, tile_y);
}

pub fn get_areas(
    image_min_coordinate: GeoCoordinate,
    image_max_coordinate: GeoCoordinate,
    image_size: Size,
    tile_min_coordinate: GeoCoordinate,
    tile_max_coordinate: GeoCoordinate,
    tile_size: Size,
) -> Option<(Area, Area)> {
    // Read from input GeoTIFF in pixels
    let read_pos_min_x = image_size.width
        * (tile_min_coordinate.x - image_min_coordinate.x)
        / (image_max_coordinate.x - image_min_coordinate.x);
    let read_pos_max_x = image_size.width
        * (tile_max_coordinate.x - image_min_coordinate.x)
        / (image_max_coordinate.x - image_min_coordinate.x);
    let read_pos_min_y = image_size.height
        - image_size.height
            * (tile_max_coordinate.y - image_min_coordinate.y)
            / (image_max_coordinate.y - image_min_coordinate.y);
    let read_pos_max_y = image_size.height
        - image_size.height
            * (tile_min_coordinate.y - image_min_coordinate.y)
            / (image_max_coordinate.y - image_min_coordinate.y);

    // Clamp values to the image boundaries
    let read_pos_min_x = read_pos_min_x.clamp(0.0, image_size.width);
    let read_pos_max_x = read_pos_max_x.clamp(0.0, image_size.width);
    let read_pos_min_y = read_pos_min_y.clamp(0.0, image_size.height);
    let read_pos_max_y = read_pos_max_y.clamp(0.0, image_size.height);

    // Determine tile's borders in pixels
    let tile_pix_min_x = match read_pos_min_x.partial_cmp(&0.0) {
        Some(Ordering::Equal) => image_min_coordinate.x,
        _ if read_pos_min_x == image_size.width => image_max_coordinate.x,
        _ => tile_min_coordinate.x,
    };
    let tile_pix_max_x = match read_pos_max_x.partial_cmp(&0.0) {
        Some(Ordering::Equal) => image_min_coordinate.x,
        _ if read_pos_max_x == image_size.width => image_max_coordinate.x,
        _ => tile_max_coordinate.x,
    };
    let tile_pix_min_y = match read_pos_max_y.partial_cmp(&0.0) {
        Some(Ordering::Equal) => image_max_coordinate.y,
        _ if read_pos_max_y == image_size.height => image_min_coordinate.y,
        _ => tile_min_coordinate.y,
    };
    let tile_pix_max_y = match read_pos_min_y.partial_cmp(&0.0) {
        Some(Ordering::Equal) => image_max_coordinate.y,
        _ if read_pos_min_y == image_size.height => image_min_coordinate.y,
        _ => tile_max_coordinate.y,
    };

    // Positions of dataset to write in tile
    let write_pos_min_x = tile_size.width
        - tile_size.width * (tile_max_coordinate.x - tile_pix_min_x)
            / (tile_max_coordinate.x - tile_min_coordinate.x);
    let write_pos_max_x = tile_size.width
        - tile_size.width * (tile_max_coordinate.x - tile_pix_max_x)
            / (tile_max_coordinate.x - tile_min_coordinate.x);
    let write_pos_min_y = tile_size.height
        * (tile_max_coordinate.y - tile_pix_max_y)
        / (tile_max_coordinate.y - tile_min_coordinate.y);
    let write_pos_max_y = tile_size.height
        * (tile_max_coordinate.y - tile_pix_min_y)
        / (tile_max_coordinate.y - tile_min_coordinate.y);

    // Sizes to read and write
    let mut read_x_size = read_pos_max_x - read_pos_min_x;
    let mut write_x_size = write_pos_max_x - write_pos_min_x;
    let mut read_y_size = (read_pos_max_y - read_pos_min_y).abs();
    let mut write_y_size = (write_pos_max_y - write_pos_min_y).abs();

    // Shifts
    let read_x_shift = read_pos_min_x.fract();
    read_x_size += read_x_shift;
    let read_y_shift = read_pos_min_y.fract();
    read_y_size += read_y_shift;
    let write_x_shift = write_pos_min_x.fract();
    write_x_size += write_x_shift;
    let write_y_shift = write_pos_min_y.fract();
    write_y_size += write_y_shift;

    // Ensure output image sides are at least 1x1 pixels
    write_x_size = write_x_size.max(1.0);
    write_y_size = write_y_size.max(1.0);

    // Return areas if valid
    if read_x_size < 1.0 || read_y_size < 1.0 || write_x_size < 1.0 || write_y_size < 1.0 {
        return None;
    }

    let read_origin_coordinate = PixelCoordinate {
        x: read_pos_min_x,
        y: read_pos_min_y,
    };
    let write_origin_coordinate = PixelCoordinate {
        x: write_pos_min_x,
        y: write_pos_min_y,
    };

    let read_area = Area {
        origin: read_origin_coordinate,
        size: Size {
            width: read_x_size.floor(),
            height: read_y_size.floor(),
        },
    };
    let write_area = Area {
        origin: write_origin_coordinate,
        size: Size {
            width: write_x_size.floor(),
            height: write_y_size.floor(),
        },
    };

    Some((read_area, write_area))
}

fn main() {
    let ds = Dataset::open("/home/gigas/documents/repos/geotiles-rs/data.tif").unwrap();
    let size = ds.raster_size();
    let coords = ds.geo_transform().unwrap();

    let min_x = coords[0];
    let min_y = coords[3] - size.1 as f64 * coords[1];
    let max_x = coords[0] + size.0 as f64 * coords[1];
    let max_y = coords[3];

    let nums_10 = to_number(min_x, min_y, 10);
    let nums_16 = to_number(min_x, min_y, 16);

    println!("This {} is in '{}' and has {} bands.", ds.driver().long_name(), ds.spatial_ref().unwrap().name().unwrap(), ds.raster_count());
}

