# Chapter 6: The Storage Stack - Talking to Disks and Files

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

### The `ata::init()` Process: Detecting and Identifying a Disk

The `ata::init()` function is a carefully choreographed sequence of steps to detect and identify the primary master ATA drive. This process involves precise timing and interaction with the disk controller's I/O ports:

1.  **Software Reset**: The driver first sends a software reset command to the ATA controller. This puts the controller into a known state.
2.  **Wait for BSY**: It then enters a loop, continuously polling the status port (0x1F7) and waiting for the `BSY` (Busy) bit to clear. This indicates that the drive has finished its internal reset and is ready to receive commands.
3.  **Select Drive**: The driver selects the master drive on the primary bus.
4.  **`IDENTIFY` Command**: It sends the `IDENTIFY` command (command code `0xEC`) to the drive. This command instructs the drive to report a wealth of information about itself, such as its model number, serial number, and capabilities.
5.  **Wait and Poll for Data**: After sending the `IDENTIFY` command, the driver waits for the drive to process it. It then polls the `DRQ` (Data Request) bit on the status port, which signals that the 512-byte block of IDENTIFY data is ready to be read from the data port.
6.  **Read Data**: The driver reads the 256 words (512 bytes) of IDENTIFY data from the data port (0x1F0).
7.  **Extract Information**: Finally, it parses this raw data to extract key information, most importantly the total number of sectors on the disk (found in words 60-61 of the IDENTIFY data), which is then stored globally for use by higher-level drivers.

> **QEMU Note**: This ATA PIO driver relies on the IDE controller being available at these fixed legacy I/O ports. This is typically only true when QEMU is run with the `-M pc` machine type (which emulates an older i440FX chipset). The default `-M q35` machine type uses a more modern ICH9 chipset where the IDE controller might not be at these fixed locations. This is why `make run-bios` (which uses `-M pc`) is often required for disk access during development, while `make run-gui-x86_64` (which uses `-M q35`) might not have disk access unless explicitly configured.

## Layer 2: The FAT16 Filesystem Driver

The ATA driver, as described above, provides only raw block-level access to the disk (reading and writing sectors). However, the kernel and user programs need a more structured way to interact with storage, using concepts like files, directories, and file names. This is where the **filesystem driver** comes in. OxideOS includes a driver for the simple and widely-supported **FAT16** format, implemented in `fat.rs`.

FAT16 is a good choice for a hobby OS because its structure is relatively straightforward to understand and implement, and it's compatible with many systems.

### FAT Concepts

A FAT (File Allocation Table) filesystem organizes the disk into several key areas:

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