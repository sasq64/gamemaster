cmake_minimum_required(VERSION 3.10)
project(ZTools VERSION 1.0 LANGUAGES C)

# Set C standard
set(CMAKE_C_STANDARD 99)
set(CMAKE_C_STANDARD_REQUIRED ON)

# Build configuration
set(CMAKE_C_FLAGS_RELEASE "-O2")
set(CMAKE_C_FLAGS_DEBUG "-g -O0")

if(NOT CMAKE_BUILD_TYPE)
    set(CMAKE_BUILD_TYPE Release)
endif()

# Option for getopt detection (equivalent to CFLAGS = -DHAS_GETOPT)
option(HAS_GETOPT "System declares getopt" OFF)
if(HAS_GETOPT)
    add_definitions(-DHAS_GETOPT)
endif()

# Source files for each executable
set(CHECK_SOURCES check.c)

set(INFODUMP_SOURCES 
    infodump.c 
    showhead.c 
    showdict.c 
    showobj.c 
    showverb.c 
    txio.c 
    infinfo.c 
    symbols.c
)

set(PIX2GIF_SOURCES pix2gif.c)

set(TXD_SOURCES 
    txd.c 
    txio.c 
    showverb.c 
    infinfo.c 
    symbols.c 
    showobj.c
)

# Build executables
add_executable(check ${CHECK_SOURCES})
add_executable(infodump ${INFODUMP_SOURCES})
add_executable(pix2gif ${PIX2GIF_SOURCES})
add_executable(txd ${TXD_SOURCES})

# Man page generation (optional, requires nroff)
find_program(NROFF_EXECUTABLE nroff)
find_program(COL_EXECUTABLE col)

if(NROFF_EXECUTABLE AND COL_EXECUTABLE)
    set(MANPAGES infodump.1 inforead.1 txd.1 check.1 pix2gif.1)

    foreach(manpage ${MANPAGES})
        get_filename_component(basename ${manpage} NAME_WE)
        add_custom_command(
            OUTPUT ${basename}.man
            COMMAND ${NROFF_EXECUTABLE} -man ${CMAKE_CURRENT_SOURCE_DIR}/${manpage} | ${COL_EXECUTABLE} -b > ${basename}.man
            DEPENDS ${CMAKE_CURRENT_SOURCE_DIR}/${manpage}
            COMMENT "Generating formatted man page ${basename}.man"
        )
        list(APPEND FORMATTED_MANPAGES ${basename}.man)
    endforeach()

    add_custom_target(doc ALL DEPENDS ${FORMATTED_MANPAGES})
else()
    message(WARNING "nroff and/or col not found - man pages will not be generated")
    add_custom_target(doc)
endif()

# Install targets
install(TARGETS check infodump pix2gif txd
    RUNTIME DESTINATION bin
)

# Install man pages if available
if(NROFF_EXECUTABLE AND COL_EXECUTABLE)
    install(FILES ${CMAKE_CURRENT_BINARY_DIR}/check.man
                  ${CMAKE_CURRENT_BINARY_DIR}/infodump.man
                  ${CMAKE_CURRENT_BINARY_DIR}/pix2gif.man
                  ${CMAKE_CURRENT_BINARY_DIR}/txd.man
            DESTINATION share/man/man1
            OPTIONAL
    )
endif()

# Install original man pages
install(FILES check.1 infodump.1 inforead.1 pix2gif.1 txd.1
        DESTINATION share/man/man1
)

# Clean target equivalent (CMake handles this automatically with 'make clean')
# Custom clean target for the formatted man pages
add_custom_target(distclean
    COMMAND ${CMAKE_COMMAND} -E remove -f ${FORMATTED_MANPAGES}
    COMMENT "Removing generated files"
)
