import sys


def write_file(p: str):
    output_message = "Hello, this is the output of the script!"

    # Writing to the specified file
    with open(p, "w") as file:
        file.write(output_message)


# TODO real action here
if __name__ == "__main__":
    print("hello stdout!")
    print("hello stderr!", file=sys.stderr)
    # Check if the expected number of arguments is provided
    if len(sys.argv) != 2:
        print("Usage: python build.py <output_filename>")
        sys.exit(1)
    p = sys.argv[1]
    write_file(p)
