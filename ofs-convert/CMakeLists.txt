cmake_minimum_required(VERSION 3.0)

project(ofs-convert)

list(APPEND CMAKE_MODULE_PATH "${CMAKE_CURRENT_SOURCE_DIR}/cmake")

find_package(UUID REQUIRED)

add_executable(ofs-convert
        ext4.cpp
        ext4.h
        ext4_bg.cpp
        ext4_bg.h
        ext4_dentry.cpp
        ext4_dentry.h
        ext4_extent.cpp
        ext4_extent.h
        ext4_inode.cpp
        ext4_inode.h
        extent-allocator.cpp
        extent-allocator.h
        extent_iterator.cpp
        extent_iterator.h
        fat.cpp
        fat.h
        metadata_reader.cpp
        metadata_reader.h
        ofs-convert.cpp
        partition.cpp
        partition.h
        stream-archiver.cpp
        stream-archiver.h
        tree_builder.cpp
        tree_builder.h
        util.cpp
        util.h
        visualizer.cpp
        visualizer.h
        visualizer_types.h)

target_link_libraries(ofs-convert ${UUID_LIBRARIES})

target_include_directories(ofs-convert
        PRIVATE ${UUID_INCLUDE_DIRS})

set_target_properties(ofs-convert PROPERTIES
        CXX_STANDARD 11
        CXX_STANDARD_REQUIRED TRUE
)
