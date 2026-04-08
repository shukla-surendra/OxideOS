# 6. The Storage Stack

This document describes the storage stack in OxideOS, covering how the kernel communicates with a physical disk drive and interprets the data as a filesystem. The stack has two main layers: the low-level ATA driver that talks to the hardware, and the higher-level FAT16 driver that understands files and directories.

---

## Layer 1: The ATA PIO Driver

The `ata.rs` module contains a driver for an ATA (Advanced Technology Attachment) hard disk using PIO (Programmed I/O).

### PIO vs. DMA

*   **Programmed I/O (PIO)**: The CPU is directly involved in transferring data. To read a sector, the CPU asks the disk to prepare the data, waits for it, and then manually reads the data from the disk's I/O port word by word, copying it into memory. This is simple to implement but can be inefficient as it keeps the CPU busy.
*   **Direct Memory Access (DMA)**: The CPU tells the disk controller to transfer a block of data directly to a specific location in RAM. The CPU is then free to do other work until the transfer is complete. This is much more efficient but also more complex to set up.

OxideOS uses PIO for its simplicity.

### Interfacing with the Hardware

The driver communicates with the IDE controller by reading from and writing to a set of I/O ports. These are special addresses separate from main memory, accessed using the `in` and `out` x86 instructions. Key ports on the primary IDE bus include:

*   `0x1F0`: Data port (for reading/writing sector data).
*   `0x1F7`: Status/Command port (to check disk status or issue commands).
*   `0x1F2-0x1F6`: Ports for specifying sector count and address (LBA).

### The `init()` Process: Detecting a Disk

The `ata::init()` function is a carefully choreographed sequence to detect and identify the primary master drive:

1.  **Software Reset**: The driver sends a reset command to the controller.
2.  **Wait for BSY**: It polls the status port, waiting for the `BSY` (Busy) bit to clear, indicating the drive is ready for a command.
3.  **Select Drive**: It selects the master drive on the primary bus.
4.  **`IDENTIFY` Command**: It sends the `IDENTIFY` command (`0xEC`). This asks the drive to report its parameters.
5.  **Wait and Poll**: It waits for the drive to process the command and then polls the `DRQ` (Data Request) bit, which signals that the 512-byte block of IDENTIFY data is ready to be read.
6.  **Read Data**: The driver reads the 256 words (512 bytes) of IDENTIFY data from the data port.
7.  **Extract Info**: It extracts key information from this data, most importantly the total number of sectors on the disk (from words 60-61), which it stores globally.

> **QEMU Note**: This driver relies on the IDE controller being available at these fixed legacy I/O ports. This is only true when QEMU is run with the `-M pc` machine type. The default `-M q35` uses a different configuration where the ports are not at this fixed location, which is why `make run-bios` is required for disk access.

## Layer 2: The FAT16 Filesystem Driver

The ATA driver provides raw sector access, but the kernel needs a way to understand files and directories. This is the job of the filesystem driver. OxideOS includes a read-only driver for the simple and widely-supported **FAT16** format in `fat.rs`.

### FAT Concepts

A FAT filesystem divides the disk into a few key areas:

1.  **Boot Sector**: The very first sector, containing the **BIOS Parameter Block (BPB)**. The BPB describes the layout of the filesystem, such as bytes per sector, sectors per cluster, and the size of the FATs.
2.  **File Allocation Tables (FATs)**: This is the heart of the filesystem. The data area of the disk is divided into blocks called **clusters**. The FAT is a large table where each entry corresponds to a cluster on the disk. The entries form linked lists. To find all the clusters belonging to a file, you start at its first cluster and follow the chain of entries in the FAT until you hit an end-of-chain marker.
3.  **Root Directory**: A fixed-size area that contains entries for the files and directories in the root of the volume. Each entry contains the filename, attributes, size, and—crucially—the number of the **first cluster** of the file's data.
4.  **Data Area**: The rest of the disk, where the actual file content is stored in clusters.

### Filesystem Operations

*   **`init()` / Mount**: The `fat::init()` function "mounts" the filesystem. It reads the boot sector using `ata::read_sector`, parses the BPB to learn the disk layout, and calculates the starting LBA addresses of the root directory and data area.

*   **`open(path)`**: To open a file, the driver:
    1.  Scans the entries in the root directory area (reading sector by sector with the ATA driver).
    2.  It compares the requested filename with each entry.
    3.  When a match is found, it extracts the file size and the starting cluster number from the directory entry.
    4.  It allocates a free **file descriptor** (FD) and stores this information.

*   **`read(fd, buf)`**: To read from an open file, the driver:
    1.  Uses the FD to look up the file's current state (e.g., current cluster, file offset).
    2.  Calculates the LBA address of the required sector within the current cluster.
    3.  Calls `ata::read_sector` to read the data into a buffer.
    4.  Copies the requested data from the sector buffer into the user's buffer.
    5.  If the read operation crosses a cluster boundary, it consults the FAT to find the next cluster in the file's chain and continues reading from there.