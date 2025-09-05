cmake_minimum_required(VERSION 3.20)
project(BlockGame LANGUAGES CXX)

# ---- Build settings ----------------------------------------------------------
set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_EXPORT_COMPILE_COMMANDS ON)
set(BUILD_SHARED_LIBS OFF) # static deps for portability

# Nice warnings
if (MSVC)
  add_compile_options(/W4 /permissive- /Zc:__cplusplus)
else()
  add_compile_options(-Wall -Wextra -Wpedantic)
endif()

# ---- Dependencies: try find_package first, then FetchContent fallback --------
# We want targets: glfw; glad::glad; glm::glm

# Try vcpkg / system packages
find_package(glfw3 CONFIG QUIET)
find_package(glad CONFIG QUIET)
find_package(glm CONFIG QUIET)

include(FetchContent)

# GLM
if (NOT glm_FOUND)
  message(STATUS "GLM not found; fetching with FetchContent...")
  FetchContent_Declare(
    glm/BG_GLFW_TARGET 
    GIT_REPOSITORY https://github.com/g-truc/glm.git
    GIT_TAG        1.0.1
  )
  set(GLM_TEST_ENABLE OFF CACHE BOOL "" FORCE)
  FetchContent_MakeAvailable(glm) # defines glm::glm
endif()

# GLAD
if (NOT glad_FOUND)
  message(STATUS "GLAD not found; fetching with FetchContent...")
  # Official CMake-friendly repo
  FetchContent_Declare(
    glad
    GIT_REPOSITORY https://github.com/Dav1dde/glad.git
    GIT_TAG        v2.0.6
  )
  # Generate for OpenGL core; version 3.3 matches our shaders
  set(GLAD_PROFILE "core" CACHE STRING "" FORCE)
  set(GLAD_API "gl=3.3" CACHE STRING "" FORCE)
  set(GLAD_GENERATOR "c" CACHE STRING "" FORCE)
  FetchContent_MakeAvailable(glad) # defines glad::glad
endif()

# GLFW
if (NOT glfw3_FOUND AND NOT TARGET glfw)
  message(STATUS "GLFW not found; fetching with FetchContent...")
  FetchContent_Declare(
    glfw
    GIT_REPOSITORY https://github.com/glfw/glfw.git
    GIT_TAG        3.4
  )
  set(GLFW_BUILD_DOCS OFF CACHE BOOL "" FORCE)
  set(GLFW_BUILD_TESTS OFF CACHE BOOL "" FORCE)
  set(GLFW_BUILD_EXAMPLES OFF CACHE BOOL "" FORCE)
  FetchContent_MakeAvailable(glfw) # defines glfw
endif()

# Normalize target names:
# - glfw3 CONFIG usually gives "glfw"
# - FetchContent gives "glfw"
if (TARGET glfw)
  set(BG_GLFW_TARGET glfw)
else()
  # Some distros export glfw3::glfw
  if (TARGET glfw::glfw)
    set(BG_GLFW_TARGET glfw::glfw)
  elseif (TARGET glfw3::glfw)
    set(BG_GLFW_TARGET glfw3::glfw)
  else()
    message(FATAL_ERROR "Could not locate a GLFW target.")
  endif()
endif()

if (NOT TARGET glad::glad)
  # Some configs export "glad" instead
  if (TARGET glad)
    add_library(glad::glad ALIAS glad)
  else()
    message(FATAL_ERROR "Could not locate a GLAD target.")
  endif()
endif()

if (NOT TARGET glm::glm)
  # Some configs export "glm" instead
  if (TARGET glm)
    add_library(glm::glm INTERFACE IMPORTED)
    target_link_libraries(glm::glm INTERFACE glm)
  else()
    message(FATAL_ERROR "Could not locate a GLM target.")
  endif()
endif()

# ---- App ---------------------------------------------------------------------
add_executable(BlockGame
  src/main.cpp
)

target_link_libraries(BlockGame PRIVATE
  ${BG_GLFW_TARGET}
  glad::glad
  glm::glm
)

# On some Linux setups, link pthread/dl when using static libs (GLFW pulls as needed)
if (UNIX AND NOT APPLE)
  find_package(Threads REQUIRED)
  target_link_libraries(BlockGame PRIVATE Threads::Threads)
  # dl is commonly needed when things are mostly static
  target_link_libraries(BlockGame PRIVATE dl)
endif()

# macOS: ensure we use modern OpenGL (GLFW handles frameworks automatically)

# ---- Run-time working dir (nice for VS Code CMake Tools) ---------------------
# Put the executable next to resources later (textures, etc.)
set_target_properties(BlockGame PROPERTIES
  RUNTIME_OUTPUT_DIRECTORY "${CMAKE_BINARY_DIR}/bin"
)
