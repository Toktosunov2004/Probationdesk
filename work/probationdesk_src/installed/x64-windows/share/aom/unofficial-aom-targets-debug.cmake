#----------------------------------------------------------------
# Generated CMake target import file for configuration "Debug".
#----------------------------------------------------------------

# Commands may need to know the format version.
set(CMAKE_IMPORT_FILE_VERSION 1)

# Import target "unofficial::aom" for configuration "Debug"
set_property(TARGET unofficial::aom APPEND PROPERTY IMPORTED_CONFIGURATIONS DEBUG)
set_target_properties(unofficial::aom PROPERTIES
  IMPORTED_IMPLIB_DEBUG "${_IMPORT_PREFIX}/debug/lib/aom_dll.lib"
  IMPORTED_LOCATION_DEBUG "${_IMPORT_PREFIX}/debug/bin/aom.dll"
  )

list(APPEND _cmake_import_check_targets unofficial::aom )
list(APPEND _cmake_import_check_files_for_unofficial::aom "${_IMPORT_PREFIX}/debug/lib/aom_dll.lib" "${_IMPORT_PREFIX}/debug/bin/aom.dll" )

# Commands beyond this point should not need to know the version.
set(CMAKE_IMPORT_FILE_VERSION)
