use wave_function::Map;

fn main() {
    let map_str = "
    0 1 2 3
    4 * * 5
    6 * * 7
    ! ! ! !
    ";
    let map = Map::from_str(map_str);
    println!("{}", &map);
}
