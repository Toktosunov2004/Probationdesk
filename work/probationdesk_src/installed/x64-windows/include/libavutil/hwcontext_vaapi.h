/*
 * This file is part of FFmpeg.
 *
 * FFmpeg is free software; you can redistribute it and/or
 * modify it under the terms of the GNU Lesser General Public
 * License as published by the Free Software Foundation; either
 * version 2.1 of the License, or (at your option) any later version.
 *
 * FFmpeg is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the GNU
 * Lesser General Public License for more details.
 *
 * You should have received a copy of the GNU Lesser General Public
 * License along with FFmpeg; if not, write to the Free Software
 * Foundation, Inc., 51 Franklin Street, Fifth Floor, Boston, MA 02110-1301 USA
 */

#ifndef AVUTIL_HWCONTEXT_VAAPI_H
#define AVUTIL_HWCONTEXT_VAAPI_H

#include <va/va.h>
#include <va/va_x11.h>
#include <va/va_drm.h>
#include <va/va_str.h>


////////////////////////////////////////////////////////////
///             VAAPI dynamic load functions start
////////////////////////////////////////////////////////////

typedef struct VAAPIDynLoadFunctions {
    // Core VA functions
    VAStatus (*vaInitialize)(VADisplay dpy, int *major_version, int *minor_version);
    VAStatus (*vaTerminate)(VADisplay dpy);
    VAStatus (*vaCreateConfig)(VADisplay dpy, VAProfile profile, VAEntrypoint entrypoint,
                              VAConfigAttrib *attrib_list, int num_attribs, VAConfigID *config_id);
    VAStatus (*vaDestroyConfig)(VADisplay dpy, VAConfigID config_id);
    VAStatus (*vaCreateContext)(VADisplay dpy, VAConfigID config_id, int picture_width,
                               int picture_height, int flag, VASurfaceID *render_targets,
                               int num_render_targets, VAContextID *context);
    VAStatus (*vaDestroyContext)(VADisplay dpy, VAContextID context);
    VAStatus (*vaCreateBuffer)(VADisplay dpy, VAContextID context, VABufferType type,
                              unsigned int size, unsigned int num_elements, void *data,
                              VABufferID *buf_id);
    VAStatus (*vaDestroyBuffer)(VADisplay dpy, VABufferID buf_id);
    VAStatus (*vaMapBuffer)(VADisplay dpy, VABufferID buf_id, void **pbuf);
    VAStatus (*vaUnmapBuffer)(VADisplay dpy, VABufferID buf_id);
    VAStatus (*vaSyncSurface)(VADisplay dpy, VASurfaceID render_target);
    VAStatus (*vaGetConfigAttributes)(VADisplay dpy, VAProfile profile,
                                     VAEntrypoint entrypoint, VAConfigAttrib *attrib_list,
                                     int num_attribs);
    VAStatus (*vaCreateSurfaces)(VADisplay dpy, unsigned int format,
                                unsigned int width, unsigned int height,
                                VASurfaceID *surfaces, unsigned int num_surfaces,
                                VASurfaceAttrib *attrib_list, unsigned int num_attribs);
    VAStatus (*vaDestroySurfaces)(VADisplay dpy, VASurfaceID *surfaces, int num_surfaces);
    VAStatus (*vaBeginPicture)(VADisplay dpy, VAContextID context, VASurfaceID render_target);
    VAStatus (*vaRenderPicture)(VADisplay dpy, VAContextID context,
                               VABufferID *buffers, int num_buffers);
    VAStatus (*vaEndPicture)(VADisplay dpy, VAContextID context);
    VAStatus (*vaQueryConfigEntrypoints)(VADisplay dpy, VAProfile profile,
                                        VAEntrypoint *entrypoint_list, int *num_entrypoints);
    VAStatus (*vaQueryConfigProfiles)(VADisplay dpy, VAProfile *profile_list, int *num_profiles);
    VAStatus (*vaGetDisplayAttributes)(VADisplay dpy, VADisplayAttribute *attr_list, int num_attributes);
    const char *(*vaErrorStr)(VAStatus error_status);
    int (*vaMaxNumEntrypoints)(VADisplay dpy);
    int (*vaMaxNumProfiles)(VADisplay dpy);
    const char *(*vaQueryVendorString)(VADisplay dpy);
    VAStatus (*vaQuerySurfaceAttributes)(VADisplay dpy, VAConfigID config_id,
                                        VASurfaceAttrib *attrib_list, int *num_attribs);
    VAStatus (*vaDestroyImage)(VADisplay dpy, VAImageID image);
    VAStatus (*vaDeriveImage)(VADisplay dpy, VASurfaceID surface, VAImage *image);
    VAStatus (*vaPutImage)(VADisplay dpy, VASurfaceID surface, VAImageID image,
                          int src_x, int src_y, unsigned int src_width, unsigned int src_height,
                          int dest_x, int dest_y, unsigned int dest_width, unsigned int dest_height);
    VAStatus (*vaCreateImage)(VADisplay dpy, VAImageFormat *format, int width, int height, VAImage *image);
    VAStatus (*vaGetImage)(VADisplay dpy, VASurfaceID surface,
                          int x, int y, unsigned int width, unsigned int height,
                          VAImageID image);
    VAStatus (*vaExportSurfaceHandle)(VADisplay dpy, VASurfaceID surface_id,
                                     uint32_t mem_type, uint32_t flags,
                                     void *descriptor);
    VAStatus (*vaReleaseBufferHandle)(VADisplay dpy, VABufferID buf_id);
    VAStatus (*vaAcquireBufferHandle)(VADisplay dpy, VABufferID buf_id, 
                                     VABufferInfo *buf_info);
    VAStatus (*vaSetErrorCallback)(VADisplay dpy, VAMessageCallback callback, void *user_context);
    VAStatus (*vaSetInfoCallback)(VADisplay dpy, VAMessageCallback callback, void *user_context);
    VAStatus (*vaSetDriverName)(VADisplay dpy, const char *driver_name);
    const char *(*vaEntrypointStr)(VAEntrypoint entrypoint);
    VAStatus (*vaQueryImageFormats)(VADisplay dpy, VAImageFormat *format_list, int *num_formats);
    int (*vaMaxNumImageFormats)(VADisplay dpy);
    const char *(*vaProfileStr)(VAProfile profile);


    // Optional functions
    VAStatus (*vaSyncBuffer)(VADisplay dpy, VABufferID buf_id, uint64_t timeout_ns);

    // X11 specific functions
    VADisplay (*vaGetDisplay)(Display *dpy);

    // DRM specific functions
    VADisplay (*vaGetDisplayDRM)(int fd);



    // Library handles
    void *handle_va;
    void *handle_va_drm;
    void *handle_va_x11;
} VAAPIDynLoadFunctions;


////////////////////////////////////////////////////////////
///             VAAPI API end
////////////////////////////////////////////////////////////

/**
 * @file
 * API-specific header for AV_HWDEVICE_TYPE_VAAPI.
 *
 * Dynamic frame pools are supported, but note that any pool used as a render
 * target is required to be of fixed size in order to be be usable as an
 * argument to vaCreateContext().
 *
 * For user-allocated pools, AVHWFramesContext.pool must return AVBufferRefs
 * with the data pointer set to a VASurfaceID.
 */

enum {
    /**
     * The quirks field has been set by the user and should not be detected
     * automatically by av_hwdevice_ctx_init().
     */
    AV_VAAPI_DRIVER_QUIRK_USER_SET = (1 << 0),
    /**
     * The driver does not destroy parameter buffers when they are used by
     * vaRenderPicture().  Additional code will be required to destroy them
     * separately afterwards.
     */
    AV_VAAPI_DRIVER_QUIRK_RENDER_PARAM_BUFFERS = (1 << 1),

    /**
     * The driver does not support the VASurfaceAttribMemoryType attribute,
     * so the surface allocation code will not try to use it.
     */
    AV_VAAPI_DRIVER_QUIRK_ATTRIB_MEMTYPE = (1 << 2),

    /**
     * The driver does not support surface attributes at all.
     * The surface allocation code will never pass them to surface allocation,
     * and the results of the vaQuerySurfaceAttributes() call will be faked.
     */
    AV_VAAPI_DRIVER_QUIRK_SURFACE_ATTRIBUTES = (1 << 3),
};

/**
 * VAAPI connection details.
 *
 * Allocated as AVHWDeviceContext.hwctx
 */
typedef struct AVVAAPIDeviceContext {
    /**
     * The VADisplay handle, to be filled by the user.
     */
    VADisplay display;
    /**
     * Driver quirks to apply - this is filled by av_hwdevice_ctx_init(),
     * with reference to a table of known drivers, unless the
     * AV_VAAPI_DRIVER_QUIRK_USER_SET bit is already present.  The user
     * may need to refer to this field when performing any later
     * operations using VAAPI with the same VADisplay.
     */
    unsigned int driver_quirks;

    VAAPIDynLoadFunctions *funcs;
} AVVAAPIDeviceContext;

/**
 * VAAPI-specific data associated with a frame pool.
 *
 * Allocated as AVHWFramesContext.hwctx.
 */
typedef struct AVVAAPIFramesContext {
    /**
     * Set by the user to apply surface attributes to all surfaces in
     * the frame pool.  If null, default settings are used.
     */
    VASurfaceAttrib *attributes;
    int           nb_attributes;
    /**
     * The surfaces IDs of all surfaces in the pool after creation.
     * Only valid if AVHWFramesContext.initial_pool_size was positive.
     * These are intended to be used as the render_targets arguments to
     * vaCreateContext().
     */
    VASurfaceID     *surface_ids;
    int           nb_surfaces;
} AVVAAPIFramesContext;

/**
 * VAAPI hardware pipeline configuration details.
 *
 * Allocated with av_hwdevice_hwconfig_alloc().
 */
typedef struct AVVAAPIHWConfig {
    /**
     * ID of a VAAPI pipeline configuration.
     */
    VAConfigID config_id;
} AVVAAPIHWConfig;

#endif /* AVUTIL_HWCONTEXT_VAAPI_H */
