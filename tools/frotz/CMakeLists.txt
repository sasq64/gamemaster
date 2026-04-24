cmake_minimum_required(VERSION 3.15)
project(frotz C)
set(CMAKE_EXPORT_COMPILE_COMMANDS 1)

set(CMAKE_C_STANDARD 99)
set(CMAKE_C_STANDARD_REQUIRED ON)
set(CMAKE_POSITION_INDEPENDENT_CODE ON)

if(NOT CMAKE_BUILD_TYPE)
  set(CMAKE_BUILD_TYPE Release)
endif()

set(FROTZ_VERSION "2.56pre")
set(FROTZ_RELEASE_NOTES "Development release.")

set(COMMON_DIR ${CMAKE_CURRENT_SOURCE_DIR}/src/common)
set(DUMB_DIR   ${CMAKE_CURRENT_SOURCE_DIR}/src/dumb)
set(BLORB_DIR  ${CMAKE_CURRENT_SOURCE_DIR}/src/blorb)

# Generate src/common/defs.h
file(WRITE ${COMMON_DIR}/defs.h
"#ifndef COMMON_DEFINES_H
#define COMMON_DEFINES_H
#define VERSION \"${FROTZ_VERSION}\"
#define RELEASE_NOTES \"${FROTZ_RELEASE_NOTES}\"
#define UNIX
#define MAX_UNDO_SLOTS 500
#define MAX_FILE_NAME 80
#define TEXT_BUFFER_SIZE 512
#define INPUT_BUFFER_SIZE 200
#define STACK_SIZE 1024
#define USE_UTF8
#endif /* COMMON_DEFINES_H */
")

# Generate src/common/hash.h (best-effort git info)
find_package(Git QUIET)
set(GIT_HASH "")
set(GIT_HASH_SHORT "")
set(GIT_DATE "")
if(Git_FOUND AND EXISTS ${CMAKE_CURRENT_SOURCE_DIR}/.git)
  execute_process(COMMAND ${GIT_EXECUTABLE} rev-parse HEAD
        WORKING_DIRECTORY ${CMAKE_CURRENT_SOURCE_DIR}
        OUTPUT_VARIABLE GIT_HASH OUTPUT_STRIP_TRAILING_WHITESPACE ERROR_QUIET)
  execute_process(COMMAND ${GIT_EXECUTABLE} rev-parse --short=8 HEAD
        WORKING_DIRECTORY ${CMAKE_CURRENT_SOURCE_DIR}
        OUTPUT_VARIABLE GIT_HASH_SHORT OUTPUT_STRIP_TRAILING_WHITESPACE ERROR_QUIET)
  execute_process(COMMAND ${GIT_EXECUTABLE} show -s --format=%ci
        WORKING_DIRECTORY ${CMAKE_CURRENT_SOURCE_DIR}
        OUTPUT_VARIABLE GIT_DATE OUTPUT_STRIP_TRAILING_WHITESPACE ERROR_QUIET)
endif()

file(WRITE ${COMMON_DIR}/hash.h
"#ifndef VERSION
#define VERSION \"${FROTZ_VERSION}\"
#endif
#ifndef RELEASE_NOTES
#define RELEASE_NOTES \"${FROTZ_RELEASE_NOTES}\"
#endif
#define GIT_HASH \"${GIT_HASH}\"
#define GIT_HASH_SHORT \"${GIT_HASH_SHORT}\"
#define GIT_DATE \"${GIT_DATE}\"
")

# Flags observed from `make dfrotz`: -Wall -std=c99 -O3 -g -D_POSIX_C_SOURCE=200809L -fPIC -fpic
add_compile_options(-Wall -g)
add_compile_definitions(_POSIX_C_SOURCE=200809L)

# frotz_common: src/common/*.c except main.c (main.c is linked into the executable directly)
add_library(frotz_common STATIC
    ${COMMON_DIR}/buffer.c
    ${COMMON_DIR}/err.c
    ${COMMON_DIR}/fastmem.c
    ${COMMON_DIR}/files.c
    ${COMMON_DIR}/getopt.c
    ${COMMON_DIR}/hotkey.c
    ${COMMON_DIR}/input.c
    ${COMMON_DIR}/math.c
    ${COMMON_DIR}/missing.c
    ${COMMON_DIR}/object.c
    ${COMMON_DIR}/process.c
    ${COMMON_DIR}/quetzal.c
    ${COMMON_DIR}/random.c
    ${COMMON_DIR}/redirect.c
    ${COMMON_DIR}/screen.c
    ${COMMON_DIR}/sound.c
    ${COMMON_DIR}/stream.c
    ${COMMON_DIR}/table.c
    ${COMMON_DIR}/text.c
    ${COMMON_DIR}/variable.c
)
target_include_directories(frotz_common PUBLIC ${COMMON_DIR})

# frotz_dumb
add_library(frotz_dumb STATIC
    ${DUMB_DIR}/dblorb.c
    ${DUMB_DIR}/dinit.c
    ${DUMB_DIR}/dinput.c
    ${DUMB_DIR}/doutput.c
    ${DUMB_DIR}/dpic.c
)
target_include_directories(frotz_dumb PUBLIC ${DUMB_DIR})
target_link_libraries(frotz_dumb PUBLIC frotz_common)

# blorblib (compiled with -I../common)
add_library(blorblib STATIC
    ${BLORB_DIR}/blorblib.c
)
target_include_directories(blorblib PUBLIC ${BLORB_DIR})
target_link_libraries(blorblib PUBLIC frotz_common)

# dfrotz: main.c + the three archives
add_executable(dfrotz ${COMMON_DIR}/main.c)
target_link_libraries(dfrotz PRIVATE frotz_common frotz_dumb blorblib)

install(TARGETS dfrotz RUNTIME DESTINATION bin)
