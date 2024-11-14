import os
import sys
import time
import requests

def watch_directory(directory: str, cb):
    # Set to track the already seen files
    seen_files = set(os.listdir(directory))

    while True:
        try:
            # Get the current list of files
            current_files = set(os.listdir(directory))
            # Find new files by comparing with the already seen files
            new_files = current_files - seen_files

            # Print the new files if any
            if new_files:
                for new_file in new_files:
                    cb(directory, new_file)

            # Update the set of seen files
            seen_files = current_files

            # Sleep for 1 second before checking again
            time.sleep(1)

        except KeyboardInterrupt:
            print("Watching stopped.")
            break
        except Exception as e:
            print(f"Error: {e}")
            break


def upload_file(filename: str, filetype: str, contents: bytes):
    print(f"uploading {filename} with type {filetype}")
    # TODO
    print(f"uploaded {filename} with type {filetype}")


def on_new_file(directory: str, new_file: str):
    print(f"New file detected: {directory}/{new_file}")
    with open(os.path.join(directory, new_file), 'rb') as file:
        upload_file(filename=new_file, filetype='img/jpeg', contents=file.read())


if __name__ == "__main__":
    # Check if directory argument is provided
    if len(sys.argv) != 2:
        print("Usage: python watcher.py <directory>")
        sys.exit(1)

    # Get the directory from command-line arguments
    directory = sys.argv[1]

    # Validate if the given path is a directory
    if not os.path.isdir(directory):
        print(f"Error: {directory} is not a valid directory.")
        sys.exit(1)

    print(f"Watching directory: {directory}")

    watch_directory(directory, on_new_file)

