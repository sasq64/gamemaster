cmake_minimum_required(VERSION 3.10)
set(CMAKE_EXPORT_COMPILE_COMMANDS 1)

project(magnetic C)

# Set C standard
set(CMAKE_C_STANDARD 99)
set(CMAKE_C_STANDARD_REQUIRED ON)

# Include directories
include_directories(Generic)

# Source files from Generic directory
set(GENERIC_SOURCES
    Talkie/emu.c
    Talkie/main.c
)

# Create executable
add_executable(magnetic
    ${GENERIC_SOURCES}
)

# Compiler flags
if(CMAKE_C_COMPILER_ID MATCHES "GNU|Clang")
  target_compile_options(magnetic PRIVATE -Wall -Wextra)
endif()

# Platform-specific definitions
if(APPLE)
  target_compile_definitions(magnetic PRIVATE __unix__)
  target_compile_options(magnetic PRIVATE -Wno-pointer-sign)
endif()

# Optional: Set output directory
set_target_properties(magnetic PROPERTIES
    RUNTIME_OUTPUT_DIRECTORY ${CMAKE_BINARY_DIR}/bin
)

install(TARGETS magnetic RUNTIME DESTINATION bin)
