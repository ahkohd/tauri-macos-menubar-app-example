use objc2_app_kit::NSBezierPath;
use tauri_nspanel::{
    cocoa::{
        appkit::{CGFloat, NSViewHeightSizable, NSViewWidthSizable, NSWindowOrderingMode},
        base::id,
        foundation::{NSPoint, NSRect, NSSize},
    },
    objc::{
        class,
        declare::ClassDecl,
        msg_send,
        runtime::{Class, Object, Sel},
        sel, sel_impl, Message,
    },
    objc_foundation::INSObject,
    objc_id::Id,
};

use objc2_foundation::{CGPoint, CGRect, CGSize, NSEdgeInsets, NSEdgeInsetsZero};

const CLS_NAME: &str = "PopoverView";

#[link(name = "Foundation", kind = "framework")]
extern "C" {}

#[allow(dead_code)]
pub struct PopoverConfig {
    pub popover_to_status_item_margin: CGFloat,
    pub background_color: id,
    pub border_color: id,
    pub border_width: CGFloat,
    pub arrow_height: CGFloat,
    pub arrow_width: CGFloat,
    pub arrow_position: CGFloat,
    pub corner_radius: CGFloat,
    pub content_edge_insets: NSEdgeInsets,
    pub right_edge_margin: CGFloat,
}

impl Default for PopoverConfig {
    fn default() -> Self {
        let background_color: id = unsafe { msg_send![class!(NSColor), windowBackgroundColor] };

        let border_color: id = unsafe { msg_send![class!(NSColor), whiteColor] };

        let border_color: id = unsafe { msg_send![border_color, colorWithAlphaComponent: 0.1] };

        Self {
            popover_to_status_item_margin: 2.0,
            background_color,
            border_color,
            border_width: 2.0,
            arrow_width: 62.0,
            arrow_height: 12.0,
            arrow_position: 0.0,
            corner_radius: 12.0,
            content_edge_insets: unsafe { NSEdgeInsetsZero },
            right_edge_margin: 12.0,
        }
    }
}

pub struct PopoverView;

unsafe impl Sync for PopoverView {}

unsafe impl Send for PopoverView {}

impl PopoverView {
    fn define_class() -> &'static Class {
        let mut decl = ClassDecl::new(CLS_NAME, class!(NSView))
            .unwrap_or_else(|| panic!("Unable to register {} class", CLS_NAME));

        decl.add_ivar::<CGFloat>("popover_to_status_item_margin");

        decl.add_ivar::<id>("background_color");

        decl.add_ivar::<id>("border_color");

        decl.add_ivar::<CGFloat>("border_width");

        decl.add_ivar::<CGFloat>("arrow_height");

        decl.add_ivar::<CGFloat>("arrow_width");

        decl.add_ivar::<CGFloat>("arrow_position");

        decl.add_ivar::<CGFloat>("corner_radius");

        decl.add_ivar::<id>("content_edge_insets");

        decl.add_ivar::<CGFloat>("right_edge_margin");

        unsafe {
            decl.add_method(
                sel!(drawRect:),
                Self::draw_rect as extern "C" fn(&Object, _, NSRect),
            );

            decl.add_method(
                sel!(setPopoverToStatusItemMargin:),
                Self::handle_set_popover_to_status_item_margin
                    as extern "C" fn(&mut Object, Sel, CGFloat),
            );

            decl.add_method(
                sel!(setBackgroundColor:),
                Self::handle_set_background_color as extern "C" fn(&mut Object, Sel, id),
            );

            decl.add_method(
                sel!(setBorderColor:),
                Self::handle_set_border_color as extern "C" fn(&mut Object, Sel, id),
            );

            decl.add_method(
                sel!(setBorderWidth:),
                Self::handle_set_border_width as extern "C" fn(&mut Object, Sel, CGFloat),
            );

            decl.add_method(
                sel!(setArrowHeight:),
                Self::handle_set_arrow_height as extern "C" fn(&mut Object, Sel, CGFloat),
            );

            decl.add_method(
                sel!(setArrowWidth:),
                Self::handle_set_arrow_width as extern "C" fn(&mut Object, Sel, CGFloat),
            );

            decl.add_method(
                sel!(setArrowPosition:),
                Self::handle_set_arrow_position as extern "C" fn(&mut Object, Sel, CGFloat),
            );

            decl.add_method(
                sel!(setCornerRadius:),
                Self::handle_set_corner_radius as extern "C" fn(&mut Object, Sel, CGFloat),
            );

            decl.add_method(
                sel!(setContentEdgeInsets:),
                Self::handle_set_content_edge_insets as extern "C" fn(&mut Object, Sel, id),
            );

            decl.add_method(
                sel!(setRightEdgeMargin:),
                Self::handle_set_right_edge_margin as extern "C" fn(&mut Object, Sel, CGFloat),
            );
        }

        decl.register()
    }

    extern "C" fn handle_set_popover_to_status_item_margin(
        this: &mut Object,
        _: Sel,
        value: CGFloat,
    ) {
        unsafe { this.set_ivar::<CGFloat>("popover_to_status_item_margin", value) };
    }

    extern "C" fn handle_set_background_color(this: &mut Object, _: Sel, ns_color: id) {
        unsafe { this.set_ivar::<id>("background_color", ns_color) };
    }

    extern "C" fn handle_set_border_color(this: &mut Object, _: Sel, ns_color: id) {
        unsafe { this.set_ivar::<id>("border_color", ns_color) };
    }

    extern "C" fn handle_set_border_width(this: &mut Object, _: Sel, value: CGFloat) {
        unsafe { this.set_ivar::<CGFloat>("border_width", value) };
    }

    extern "C" fn handle_set_arrow_height(this: &mut Object, _: Sel, value: CGFloat) {
        unsafe { this.set_ivar::<CGFloat>("arrow_height", value) };
    }

    extern "C" fn handle_set_arrow_width(this: &mut Object, _: Sel, value: CGFloat) {
        unsafe { this.set_ivar::<CGFloat>("arrow_width", value) };
    }

    extern "C" fn handle_set_arrow_position(this: &mut Object, _: Sel, value: CGFloat) {
        unsafe { this.set_ivar::<CGFloat>("arrow_position", value) };
    }

    extern "C" fn handle_set_corner_radius(this: &mut Object, _: Sel, value: CGFloat) {
        unsafe { this.set_ivar::<CGFloat>("corner_radius", value) };
    }

    extern "C" fn handle_set_content_edge_insets(this: &mut Object, _: Sel, ns_edge_insets: id) {
        unsafe { this.set_ivar::<id>("content_edge_insets", ns_edge_insets) };
    }

    extern "C" fn handle_set_right_edge_margin(this: &mut Object, _: Sel, value: CGFloat) {
        unsafe { this.set_ivar::<CGFloat>("right_edge_margin", value) };
    }

    extern "C" fn draw_rect(this: &Object, _: Sel, rect: NSRect) {
        let arrow_height = unsafe { this.get_ivar::<CGFloat>("arrow_height") };

        let arrow_width = unsafe { this.get_ivar::<CGFloat>("arrow_width") };

        let arrow_position = unsafe { this.get_ivar::<CGFloat>("arrow_position") };

        let border_width = unsafe { this.get_ivar::<CGFloat>("border_width") };

        let corner_radius = unsafe { this.get_ivar::<CGFloat>("corner_radius") };

        let border_color = unsafe { this.get_ivar::<id>("border_color") };

        let bg_color = unsafe { this.get_ivar::<id>("background_color") };

        let bg_rect = NSRect::new(
            rect.origin,
            NSSize::new(rect.size.width, rect.size.height - arrow_height),
        );

        let bg_rect = NSRect::new(
            NSPoint::new(
                bg_rect.origin.x + border_width,
                bg_rect.origin.y + border_width,
            ),
            NSSize::new(
                bg_rect.size.width - 2.0 * border_width,
                bg_rect.size.height - 2.0 * border_width,
            ),
        );

        let control_points = unsafe { NSBezierPath::new() };

        let window_path = unsafe { NSBezierPath::new() };

        let arrow_path = unsafe { NSBezierPath::new() };

        let bg_path = unsafe { NSBezierPath::new() };

        unsafe {
            bg_path.appendBezierPathWithRoundedRect_xRadius_yRadius(
                CGRect::new(
                    CGPoint::new(bg_rect.origin.x, bg_rect.origin.y),
                    CGSize::new(bg_rect.size.width, bg_rect.size.height),
                ),
                *corner_radius,
                *corner_radius,
            )
        };

        let x = *arrow_position;

        let y_max = bg_rect.origin.y + bg_rect.size.height;

        let left_point = NSPoint::new(x - arrow_width / 2.0, y_max);

        let to_point = NSPoint::new(x, y_max + arrow_height);

        let right_point = NSPoint::new(x + arrow_width / 2.0, y_max);

        let cp11 = NSPoint::new(x - arrow_width / 6.0, y_max);

        let cp12 = NSPoint::new(x - arrow_width / 9.0, y_max + arrow_height);

        let cp21 = NSPoint::new(x + arrow_width / 9.0, y_max + arrow_height);

        let cp22 = NSPoint::new(x + arrow_width / 6.0, y_max);

        unsafe {
            control_points.appendBezierPathWithOvalInRect(CGRect::new(
                CGPoint::new(left_point.x - 2.0, left_point.y - 2.0),
                CGSize::new(4.0, 4.0),
            ));
        }

        unsafe {
            control_points.appendBezierPathWithOvalInRect(CGRect::new(
                CGPoint::new(to_point.x - 2.0, to_point.y - 2.0),
                CGSize::new(4.0, 4.0),
            ));
        }

        unsafe {
            control_points.appendBezierPathWithOvalInRect(CGRect::new(
                CGPoint::new(right_point.x - 2.0, right_point.y - 2.0),
                CGSize::new(4.0, 4.0),
            ));
        }

        unsafe {
            control_points.appendBezierPathWithOvalInRect(CGRect::new(
                CGPoint::new(cp11.x - 2.0, cp11.y - 2.0),
                CGSize::new(4.0, 4.0),
            ));
        }

        unsafe {
            control_points.appendBezierPathWithOvalInRect(CGRect::new(
                CGPoint::new(cp12.x - 2.0, cp12.y - 2.0),
                CGSize::new(4.0, 4.0),
            ));
        }

        unsafe {
            control_points.appendBezierPathWithOvalInRect(CGRect::new(
                CGPoint::new(cp21.x - 2.0, cp21.y - 2.0),
                CGSize::new(4.0, 4.0),
            ));
        }

        unsafe {
            control_points.appendBezierPathWithOvalInRect(CGRect::new(
                CGPoint::new(cp22.x - 2.0, cp22.y - 2.0),
                CGSize::new(4.0, 4.0),
            ));
        }

        unsafe {
            arrow_path.moveToPoint(CGPoint::new(left_point.x, left_point.y));
        }

        unsafe {
            arrow_path.curveToPoint_controlPoint1_controlPoint2(
                CGPoint::new(to_point.x, to_point.y),
                CGPoint::new(cp11.x, cp11.y),
                CGPoint::new(cp12.x, cp12.y),
            );
        }

        unsafe {
            arrow_path.curveToPoint_controlPoint1_controlPoint2(
                CGPoint::new(right_point.x, right_point.y),
                CGPoint::new(cp21.x, cp21.y),
                CGPoint::new(cp22.x, cp22.y),
            );
        }

        unsafe {
            arrow_path.lineToPoint(CGPoint::new(left_point.x, left_point.y));
        }

        unsafe {
            arrow_path.closePath();
        }

        unsafe {
            window_path.appendBezierPath(&arrow_path);
            window_path.appendBezierPath(&bg_path);
        }

        if !border_color.is_null() {
            let () = unsafe { msg_send![*border_color, setStroke] };

            unsafe {
                window_path.setLineWidth(*border_width);

                window_path.stroke();
            };
        }

        let () = unsafe { msg_send![*bg_color, setFill] };

        unsafe {
            window_path.fill();
        }
    }

    pub fn new(config: PopoverConfig) -> Id<PopoverView> {
        let popover_view: id = unsafe { msg_send![Self::class(), alloc] };

        let popover_view: id = unsafe { msg_send![popover_view, init] };

        let () = unsafe {
            msg_send![popover_view, setPopoverToStatusItemMargin: config.popover_to_status_item_margin ]
        };

        let () = unsafe { msg_send![popover_view, setBackgroundColor: config.background_color] };

        let () = unsafe { msg_send![popover_view, setBorderColor: config.border_color] };

        let () = unsafe { msg_send![popover_view, setBorderWidth: config.border_width ] };

        let () = unsafe { msg_send![popover_view, setArrowHeight: config.arrow_height ] };

        let () = unsafe { msg_send![popover_view, setArrowWidth: config.arrow_width ] };

        let () = unsafe { msg_send![popover_view, setArrowPosition: config.arrow_position ] };

        let () = unsafe { msg_send![popover_view, setCornerRadius: config.corner_radius] };

        let () =
            unsafe { msg_send![popover_view, setContentEdgeInsets: config.content_edge_insets ] };

        let () = unsafe { msg_send![popover_view, setRightEdgeMargin: config.right_edge_margin ] };

        let popover_view = unsafe { Id::from_retained_ptr(popover_view as *mut PopoverView) };

        popover_view
    }

    pub fn set_frame(&self, frame: NSRect) {
        unsafe {
            let () = msg_send![self, setFrame: frame];
        }
    }

    pub fn set_parent(&self, parent_view: id) {
        let () = unsafe {
            msg_send![parent_view, addSubview: self positioned: NSWindowOrderingMode::NSWindowBelow relativeTo: 0]
        };
    }

    pub fn set_autoresizing(&self) {
        let autoresizing_mask = NSViewWidthSizable | NSViewHeightSizable;

        let () = unsafe { msg_send![self, setAutoresizingMask: autoresizing_mask] };
    }
}

unsafe impl Message for PopoverView {}

impl INSObject for PopoverView {
    fn class() -> &'static Class {
        Class::get(CLS_NAME).unwrap_or_else(Self::define_class)
    }
}
