cmake_minimum_required(VERSION 3.10)
project(level9)
set(CMAKE_EXPORT_COMPILE_COMMANDS 1)

set(SOURCES
    bitmap.c
    level9.c
    talkie.c
)

add_executable(level9 ${SOURCES})
target_link_libraries(level9 PRIVATE m)

install(TARGETS level9 RUNTIME DESTINATION bin)
