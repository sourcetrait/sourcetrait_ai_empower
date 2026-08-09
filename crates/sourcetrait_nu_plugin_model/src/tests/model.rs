use crate::*;

#[test]
fn test_basic_shape() {
    dbg!(NuModel::parse("record<file: path, dir: directory, name: string>").unwrap());
    dbg!(NuModel::parse(r#"
        # This is a model summary line.
        #
        # This is a detailed description.
        # And this is more detail.
        @namepath vocab/test/basic/Shape
        @version 0.0.1-test
        record<
            file: path,  # this is a file
            dir: directory,  # this is a directory
            num: float,  # this is a number
            # this is a tab summary
            tab: table<  # this is a tab summary continued
                key: string,  # this is a key
                value: oneof<  # this is a value
                    int,  # this is a value integer
                    record<  # this is a value record
                        stuff: string,  # this is stuff
                        intg: int,  # this is another integer
                        subtab: table<  # this is a subtab
                            k: string,  # this is a subtab key
                            v: directory,  # this is a subtab value
                        >,
                    >,
                >,
            >,
        >"#).unwrap()
    );
}
